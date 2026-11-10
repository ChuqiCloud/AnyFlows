use super::*;

pub(super) async fn channel_config_row<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
) -> Result<Option<QueryResult>, UpstreamUsageError> {
    let q = Query::select()
        .columns(["config_json", "operator_user_id"].map(Alias::new))
        .from(Alias::new("channel_usage_configs"))
        .and_where(Expr::col(Alias::new("channel_id")).eq(channel.get()))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&q))
        .await
        .map_err(internal)
}

impl UpstreamUsageRepository {
    pub async fn channel_config(
        &self,
        channel: ChannelId,
    ) -> Result<UpstreamUsageConfig, UpstreamUsageError> {
        self.bounded(async {
            if channels::Entity::find_by_id(channel.get())
                .filter(channels::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(UpstreamUsageError::NotFound);
            }
            match channel_config_row(self.pool.connection(), channel).await? {
                Some(row) => serde_json::from_str(
                    &row.try_get::<String>("", "config_json").map_err(internal)?,
                )
                .map_err(internal),
                None => Ok(UpstreamUsageConfig::default()),
            }
        })
        .await
    }

    pub async fn save_channel_config(
        &self,
        actor: UserId,
        channel: ChannelId,
        mut config: UpstreamUsageConfig,
    ) -> Result<UpstreamUsageConfig, UpstreamUsageError> {
        config.validate()?;
        config.inherited = false;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let lock = Query::update()
                .table(channels::Entity)
                .value(channels::Column::Id, channel.get())
                .and_where(Expr::col(channels::Column::Id).eq(channel.get()))
                .and_where(Expr::col(channels::Column::DeletedAt).is_null())
                .to_owned();
            txn.execute(txn.get_database_backend().build(&lock))
                .await
                .map_err(internal)?;
            if channels::Entity::find_by_id(channel.get())
                .filter(channels::Column::DeletedAt.is_null())
                .one(&txn)
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(UpstreamUsageError::NotFound);
            }
            let previous: UpstreamUsageConfig = match channel_config_row(&txn, channel).await? {
                Some(row) => serde_json::from_str(
                    &row.try_get::<String>("", "config_json").map_err(internal)?,
                )
                .map_err(internal)?,
                None => UpstreamUsageConfig::default(),
            };
            if previous.version != config.version {
                return Err(UpstreamUsageError::Conflict);
            }
            if let Some(id) = config.profile_id {
                let row = profile_row(&txn, id)
                    .await?
                    .ok_or(UpstreamUsageError::NotFound)?;
                let profile = decode_profile(&row)?;
                if config.provider != UpstreamUsageProvider::Auto
                    && config.provider != profile.provider
                {
                    return Err(UpstreamUsageError::InvalidInput);
                }
            }
            config.version += 1;
            let insert = Query::insert()
                .into_table(Alias::new("channel_usage_configs"))
                .columns(
                    [
                        "channel_id",
                        "profile_id",
                        "config_json",
                        "version",
                        "operator_user_id",
                    ]
                    .map(Alias::new),
                )
                .values_panic([
                    channel.get().into(),
                    config.profile_id.into(),
                    serde_json::to_string(&config).map_err(internal)?.into(),
                    config.version.into(),
                    actor.get().into(),
                ])
                .on_conflict(
                    OnConflict::column(Alias::new("channel_id"))
                        .update_columns(
                            ["profile_id", "config_json", "version", "operator_user_id"]
                                .map(Alias::new),
                        )
                        .to_owned(),
                )
                .to_owned();
            txn.execute(txn.get_database_backend().build(&insert))
                .await
                .map_err(internal)?;
            clear_channel_runtime(&txn, channel).await?;
            txn.commit().await.map_err(internal)?;
            Ok(config)
        })
        .await
    }

    pub async fn reset_config(
        &self,
        channel: ChannelId,
        credential: CredentialId,
    ) -> Result<(), UpstreamUsageError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let lock = Query::update()
                .table(credentials::Entity)
                .value(credentials::Column::Id, credential.get())
                .and_where(Expr::col(credentials::Column::Id).eq(credential.get()))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&lock))
                .await
                .map_err(internal)?;
            load_target(&txn, channel, credential).await?;
            let delete = Query::delete()
                .from_table(Alias::new("credential_usage_configs"))
                .and_where(Expr::col(Alias::new("credential_id")).eq(credential.get()))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&delete))
                .await
                .map_err(internal)?;
            clear_channel_runtime(&txn, channel).await?;
            txn.commit().await.map_err(internal)?;
            Ok(())
        })
        .await
    }
}

pub(super) async fn clear_channel_runtime<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
) -> Result<(), UpstreamUsageError> {
    let update = Query::update()
        .table(Alias::new("credential_usage_pauses"))
        .value(Alias::new("pause_until"), Option::<i64>::None)
        .value(Alias::new("pause_identity"), Option::<String>::None)
        .value(Alias::new("next_refresh_at"), 0_i64)
        .and_where(Expr::col(Alias::new("channel_id")).eq(channel.get()))
        .to_owned();
    connection
        .execute(connection.get_database_backend().build(&update))
        .await
        .map_err(internal)?;
    crate::scheduler_outbox::enqueue_scheduler_catalog_change(
        connection,
        crate::SchedulerCatalogSubject::Channel(channel),
        crate::DatabaseTimestamp::now_utc(),
    )
    .await
    .map_err(internal)
}
