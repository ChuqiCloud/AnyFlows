use super::*;

/// Enumerate only the keys actually bound to this account profile, including channel inheritance.
pub(super) async fn usage_bindings<C: ConnectionTrait>(
    connection: &C,
    profile: Option<i64>,
    credential: Option<CredentialId>,
) -> Result<Vec<(ChannelId, CredentialId)>, UpstreamUsageError> {
    let key_profile = Query::select()
        .column(Alias::new("credential_id"))
        .from(Alias::new("credential_usage_configs"))
        .and_where(Expr::col(Alias::new("profile_id")).eq(profile))
        .to_owned();
    let channel_profile = Query::select()
        .column(Alias::new("channel_id"))
        .from(Alias::new("channel_usage_configs"))
        .and_where(Expr::col(Alias::new("profile_id")).eq(profile))
        .to_owned();
    let overrides = Query::select()
        .column(Alias::new("credential_id"))
        .from(Alias::new("credential_usage_configs"))
        .to_owned();
    let mut query = Query::select();
    query
        .columns([credentials::Column::Id, credentials::Column::ChannelId])
        .from(credentials::Entity)
        .and_where(Expr::col(credentials::Column::DeletedAt).is_null());
    if profile.is_some() {
        query.cond_where(
            sea_orm::Condition::any()
                .add(Expr::col(credentials::Column::Id).in_subquery(key_profile))
                .add(
                    sea_orm::Condition::all()
                        .add(Expr::col(credentials::Column::ChannelId).in_subquery(channel_profile))
                        .add(Expr::col(credentials::Column::Id).not_in_subquery(overrides)),
                ),
        );
    } else if let Some(id) = credential {
        query.and_where(Expr::col(credentials::Column::Id).eq(id.get()));
    } else {
        return Ok(Vec::new());
    }
    query
        .order_by(credentials::Column::Id, sea_orm::Order::Asc)
        .limit(4097);
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(internal)?;
    // A single shared query configuration is deliberately bounded independently of total site size.
    if rows.len() > 4096 {
        return Err(UpstreamUsageError::Unavailable);
    }
    rows.into_iter()
        .map(|row| {
            Ok((
                ChannelId::new(row.try_get("", "channel_id").map_err(internal)?)
                    .map_err(internal)?,
                CredentialId::new(row.try_get("", "id").map_err(internal)?).map_err(internal)?,
            ))
        })
        .collect()
}
