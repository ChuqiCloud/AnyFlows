use super::*;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

impl MerchantPoolRepository {
    pub async fn apply(
        &self,
        owner: UserId,
        write: MerchantApplication,
    ) -> Result<MerchantProfile, MerchantPoolError> {
        if !af_domain::validate_merchant_text(&write.name, 128, false)
            || !af_domain::validate_merchant_text(&write.description, 2048, true)
        {
            return Err(MerchantPoolError::InvalidInput);
        }
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            let existing = profiles::Entity::find()
                .filter(profiles::Column::OwnerUserId.eq(owner.get()))
                .one(&txn)
                .await
                .map_err(internal)?;
            let now = DatabaseTimestamp::now_utc();
            let row = if let Some(existing) = existing {
                if existing.status != "rejected" {
                    return Err(MerchantPoolError::Conflict);
                }
                let mut active = existing.into_active_model();
                active.name = Set(write.name);
                active.description = Set(write.description);
                active.status = Set("pending".to_owned());
                active.review_reason = Set(None);
                active.updated_at = Set(now);
                active.update(&txn).await.map_err(internal)?
            } else {
                profiles::ActiveModel {
                    owner_user_id: Set(owner.get()),
                    name: Set(write.name),
                    description: Set(write.description),
                    status: Set("pending".to_owned()),
                    review_reason: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&txn)
                .await
                .map_err(internal)?
            };
            txn.commit().await.map_err(internal)?;
            Ok(profile_snapshot(row))
        })
        .await
    }

    pub async fn write_pool(
        &self,
        actor: UserId,
        id: Option<i64>,
        write: MerchantPoolWrite,
    ) -> Result<MerchantPool, MerchantPoolError> {
        write.validate()?;
        self.bounded(async {
            let txn=self.pool.connection().begin().await.map_err(internal)?;
            let group_id=GroupId::new(write.group_id).map_err(invalid)?;
            let mut group_query=groups::Entity::find_by_id(write.group_id).filter(groups::Column::DeletedAt.is_null());
            if txn.get_database_backend()!=DbBackend::Sqlite { group_query=group_query.lock(LockType::Update); }
            if group_query.one(&txn).await.map_err(internal)?.is_none() { return Err(MerchantPoolError::InvalidInput); }
            let now=DatabaseTimestamp::now_utc();
            let mut active=if let Some(id)=id {
                let current=find_pool(&txn,id).await?;
                if current.group_id!=write.group_id { return Err(MerchantPoolError::Conflict); }
                current.into_active_model()
            } else {
                if pools::Entity::find().filter(pools::Column::GroupId.eq(write.group_id)).one(&txn).await.map_err(internal)?.is_some() {
                    return Err(MerchantPoolError::Conflict);
                }
                if crate::entity::channel_groups::Entity::find().filter(crate::entity::channel_groups::Column::GroupId.eq(write.group_id))
                    .one(&txn).await.map_err(internal)?.is_some() { return Err(MerchantPoolError::Conflict); }
                pools::ActiveModel{created_at:Set(now),..Default::default()}
            };
            active.group_id=Set(group_id.get());active.name=Set(write.name);active.description=Set(write.description);
            active.enabled=Set(write.enabled);active.commission_bps=Set(i32::from(write.commission_bps));
            active.commission_inherit_global=Set(write.commission_inherit_global);
            active.min_ratio_micros=Set(write.min_ratio_micros);active.max_ratio_micros=Set(write.max_ratio_micros);
            active.settlement_delay_secs=Set(i64::from(write.settlement_delay_secs));active.updated_at=Set(now);
            let result=if id.is_some() { active.update(&txn).await } else { active.insert(&txn).await }.map_err(internal)?;
            audit_control(&txn,actor,"pool.configure",result.id,None,Some(serde_json::json!({"group_id":result.group_id,"enabled":result.enabled,
                "commission_bps":result.commission_bps,"commission_inherit_global":result.commission_inherit_global}))).await?;
            txn.commit().await.map_err(internal)?;
            self.effective_pool(result,None).await
        }).await
    }

    pub async fn review_merchant(
        &self,
        actor: UserId,
        id: i64,
        review: MerchantReview,
    ) -> Result<MerchantProfile, MerchantPoolError> {
        validate_review(&review, false)?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let row = profiles::Entity::find_by_id(id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            lock_owner(&txn, UserId::new(row.owner_user_id).map_err(internal)?).await?;
            let row = profiles::Entity::find_by_id(id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            let before = row.status.clone();
            let now = DatabaseTimestamp::now_utc();
            if review.decision == "reject" && row.status != "pending" {
                return Err(MerchantPoolError::Conflict);
            }
            if review.decision == "approve"
                && !matches!(row.status.as_str(), "pending" | "suspended")
            {
                return Err(MerchantPoolError::Conflict);
            }
            let mut active = row.into_active_model();
            active.status = Set(match review.decision.as_str() {
                "approve" => "approved",
                "reject" => "rejected",
                _ => "suspended",
            }
            .to_owned());
            active.review_reason = Set(Some(review.reason));
            active.updated_at = Set(now);
            let result = active.update(&txn).await.map_err(internal)?;
            if review.decision == "suspend" {
                let list = services::Entity::find()
                    .filter(services::Column::MerchantId.eq(id))
                    .all(&txn)
                    .await
                    .map_err(internal)?;
                for service in list {
                    if let Some(revision_id) = service.published_revision_id {
                        let revision = load_revision(&txn, revision_id, service.id).await?;
                        set_channel_enabled(&txn, revision.channel_id, false, now).await?;
                    }
                    let mut service = service.into_active_model();
                    service.status = Set("suspended".to_owned());
                    service.platform_suspended = Set(true);
                    service.updated_at = Set(now);
                    service.update(&txn).await.map_err(internal)?;
                }
            }
            audit_control(
                &txn,
                actor,
                "merchant.review",
                id,
                Some(serde_json::json!({"status":before})),
                Some(serde_json::json!({"status":result.status,"reason":result.review_reason})),
            )
            .await?;
            txn.commit().await.map_err(internal)?;
            Ok(profile_snapshot(result))
        })
        .await
    }

    /// A single transaction owns the draft channel, credential AEAD binding and revision.
    pub async fn write_service<F>(
        &self,
        owner: UserId,
        id: Option<i64>,
        write: MerchantServiceWrite,
        encrypt: F,
    ) -> Result<MerchantOwnedService, MerchantPoolError>
    where
        F: FnOnce(
                ChannelId,
                CredentialId,
                Option<MerchantCredentialMaterial>,
                Option<String>,
            ) -> Result<EncryptedCredentialEnvelope, MerchantPoolError>
            + Send,
    {
        write.validate()?;
        let mut write = write;
        self.bounded(async {
            let txn=self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn,owner).await?;
            let merchant=require_merchant(&txn,owner,true).await?;
            let pool=find_pool(&txn,write.pool_id).await?;
            if !pool.enabled { return Err(MerchantPoolError::InvalidInput); }
            validate_ratios(&pool,write.default_ratio_micros,&write.models)?;
            for model in &write.models {
                if super::catalog::model_price(&txn,&model.model,model.ratio_micros.unwrap_or(write.default_ratio_micros)).await?.is_none() {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            let now=DatabaseTimestamp::now_utc();
            let service=if let Some(id)=id {
                let service=lock_service(&txn,id).await?;
                if service.merchant_id!=merchant.id { return Err(MerchantPoolError::NotFound); }
                if revisions::Entity::find().filter(revisions::Column::ServiceId.eq(id)).count(&txn).await.map_err(internal)? >= 256 {
                    return Err(MerchantPoolError::LimitReached);
                }
                if service.pool_id!=write.pool_id || service.status=="suspended" { return Err(MerchantPoolError::Conflict); }
                if let Some(pending)=service.pending_revision_id {
                    let current=load_revision(&txn,pending,id).await?;
                    if current.status=="pending" { return Err(MerchantPoolError::Conflict); }
                }
                service
            } else {
                let count=services::Entity::find().filter(services::Column::MerchantId.eq(merchant.id)).count(&txn).await.map_err(internal)?;
                if count>=32 { return Err(MerchantPoolError::LimitReached); }
                services::ActiveModel{merchant_id:Set(merchant.id),pool_id:Set(write.pool_id),name:Set(write.name.clone()),
                    description:Set(write.description.clone()),status:Set("draft".to_owned()),published_revision_id:Set(None),pending_revision_id:Set(None),
                    created_at:Set(now),updated_at:Set(now),..Default::default()}.insert(&txn).await.map_err(internal)?
            };
            let previous_id=service.pending_revision_id.or(service.published_revision_id);
            let previous=match previous_id { Some(id)=>Some(load_revision(&txn,id,service.id).await?),None=>None };
            let previous_secret=if write.api_key.is_none() {
                let previous=previous.as_ref().ok_or(MerchantPoolError::InvalidInput)?;
                if previous.base_url.as_str()!=ChannelBaseUrl::parse(&write.base_url).map_err(invalid)?.as_str() {
                    return Err(MerchantPoolError::InvalidInput);
                }
                let credential=credentials::Entity::find_by_id(previous.credential_id)
                    .filter(credentials::Column::ChannelId.eq(previous.channel_id)).filter(credentials::Column::DeletedAt.is_null())
                    .one(&txn).await.map_err(internal)?.ok_or(MerchantPoolError::Unavailable)?;
                let (key_id,nonce,ciphertext)=credential.secret.envelope_parts().map_err(internal)?;
                Some(MerchantCredentialMaterial{channel_id:ChannelId::new(previous.channel_id).map_err(internal)?,
                    credential_id:CredentialId::new(previous.credential_id).map_err(internal)?,
                    envelope:EncryptedCredentialEnvelope::new(key_id,nonce,ciphertext).map_err(internal)?})
            } else { None };
            let mapping=write.models.iter().map(|model|(model.model.clone(),serde_json::Value::String(model.upstream_model.clone())))
                .collect::<serde_json::Map<_,_>>();
            let channel=channels::ActiveModel{name:Set(format!("merchant:{}:{}",merchant.id,service.id)),r#type:Set("openai".to_owned()),
                protocol:Set(write.protocol.clone()),base_url:Set(Some(ChannelBaseUrl::parse(&write.base_url).map_err(invalid)?)),
                timeout_secs:Set(Some(i32::try_from(write.timeout_secs).map_err(invalid)?)),status:Set(Status::Disabled.code()),
                weight:Set(1),priority:Set(0),auto_ban:Set(true),model_mapping:Set(serde_json::Value::Object(mapping)),
                param_override:Set(serde_json::json!({})),header_override:Set(HeaderOverrides::validate(serde_json::json!({})).map_err(internal)?),
                settings:Set(SensitiveJson::from(serde_json::json!({"relay_race_width":1}))),tag:Set(Some("merchant-pool".to_owned())),
                created_at:Set(now),updated_at:Set(now),..Default::default()}.insert(&txn).with_subscriber(NoSubscriber::default()).await.map_err(internal)?;
            let channel_id=ChannelId::new(channel.id).map_err(internal)?;
            let placeholder=EncryptedJson::from_envelope(serde_json::json!({"version":1,"algorithm":"xchacha20poly1305",
                "key_id":"transaction-pending","nonce":URL_SAFE_NO_PAD.encode([0_u8;24]),"ciphertext":URL_SAFE_NO_PAD.encode([0_u8;16])})).map_err(internal)?;
            let credential=credentials::ActiveModel{channel_id:Set(channel.id),kind:Set("api_key".to_owned()),secret:Set(placeholder),
                status:Set(Status::Enabled.code()),priority:Set(0),weight:Set(1),concurrency:Set(write.concurrency.map(i32::try_from).transpose().map_err(invalid)?),
                schedulable:Set(true),quota_dimension:Set("global".to_owned()),oauth_token_pending:Set(false),oauth_revision:Set(0),
                created_at:Set(now),updated_at:Set(now),..Default::default()}.insert(&txn).with_subscriber(NoSubscriber::default()).await.map_err(internal)?;
            let credential_id=CredentialId::new(credential.id).map_err(internal)?;
            let encrypted=encrypt(channel_id,credential_id,previous_secret,write.api_key.take())?;
            let encrypted=EncryptedJson::from_envelope(serde_json::json!({"version":1,"algorithm":"xchacha20poly1305",
                "key_id":encrypted.key_id(),"nonce":URL_SAFE_NO_PAD.encode(encrypted.nonce()),"ciphertext":URL_SAFE_NO_PAD.encode(encrypted.ciphertext())})).map_err(internal)?;
            let mut credential=credential.into_active_model();credential.secret=Set(encrypted);credential.update(&txn).with_subscriber(NoSubscriber::default()).await.map_err(internal)?;
            let number=previous.as_ref().map_or(Ok(1),|previous|previous.revision.checked_add(1).ok_or(MerchantPoolError::Unavailable))?;
            let revision=revisions::ActiveModel{service_id:Set(service.id),revision:Set(number),status:Set("draft".to_owned()),
                name:Set(write.name.clone()),description:Set(write.description.clone()),
                base_url:Set(ChannelBaseUrl::parse(&write.base_url).map_err(invalid)?),protocol:Set(write.protocol),default_ratio_micros:Set(write.default_ratio_micros),
                models:Set(serde_json::to_value(&write.models).map_err(internal)?),channel_id:Set(channel.id),credential_id:Set(credential_id.get()),
                concurrency:Set(write.concurrency.map(i32::try_from).transpose().map_err(invalid)?),timeout_secs:Set(write.timeout_secs.try_into().map_err(invalid)?),
                review_reason:Set(None),created_at:Set(now),updated_at:Set(now),..Default::default()}.insert(&txn).with_subscriber(NoSubscriber::default()).await.map_err(internal)?;
            if let Some(previous)=previous
                && Some(previous.id)!=service.published_revision_id {
                    let mut previous=previous.into_active_model();previous.status=Set("superseded".to_owned());previous.updated_at=Set(now);
                    previous.update(&txn).await.map_err(internal)?;
            }
            let mut service=service.into_active_model();service.pending_revision_id=Set(Some(revision.id));service.updated_at=Set(now);
            // Draft descriptions cannot replace the live reviewed projection.
            if id.is_none() { service.name=Set(write.name);service.description=Set(write.description); }
            let result=service.update(&txn).await.map_err(internal)?;
            let result=service_snapshot(&txn,result).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        }).await
    }

    pub async fn submit_service(
        &self,
        owner: UserId,
        id: i64,
    ) -> Result<MerchantOwnedService, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            let merchant = require_merchant(&txn, owner, true).await?;
            let service = lock_service(&txn, id).await?;
            if service.merchant_id != merchant.id {
                return Err(MerchantPoolError::NotFound);
            }
            if service.status == "suspended" {
                return Err(MerchantPoolError::Forbidden);
            }
            let revision = load_revision(
                &txn,
                service
                    .pending_revision_id
                    .ok_or(MerchantPoolError::Conflict)?,
                id,
            )
            .await?;
            if !matches!(revision.status.as_str(), "draft" | "rejected") {
                return Err(MerchantPoolError::Conflict);
            }
            let now = DatabaseTimestamp::now_utc();
            let mut revision = revision.into_active_model();
            revision.status = Set("pending".to_owned());
            revision.review_reason = Set(None);
            revision.updated_at = Set(now);
            revision.update(&txn).await.map_err(internal)?;
            let published = service.published_revision_id.is_some();
            let mut service = service.into_active_model();
            if !published {
                service.status = Set("pending".to_owned());
            }
            service.updated_at = Set(now);
            let result =
                service_snapshot(&txn, service.update(&txn).await.map_err(internal)?).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    pub async fn pause_service(
        &self,
        owner: UserId,
        id: i64,
        paused: bool,
    ) -> Result<MerchantOwnedService, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn, owner).await?;
            let merchant = require_merchant(&txn, owner, true).await?;
            let service = lock_service(&txn, id).await?;
            if service.merchant_id != merchant.id {
                return Err(MerchantPoolError::NotFound);
            }
            if !matches!(service.status.as_str(), "published" | "paused") {
                return Err(MerchantPoolError::Conflict);
            }
            let revision = load_revision(
                &txn,
                service
                    .published_revision_id
                    .ok_or(MerchantPoolError::Conflict)?,
                id,
            )
            .await?;
            let now = DatabaseTimestamp::now_utc();
            set_channel_enabled(&txn, revision.channel_id, !paused, now).await?;
            let mut service = service.into_active_model();
            service.merchant_paused = Set(paused);
            service.status = Set(if paused { "paused" } else { "published" }.to_owned());
            service.updated_at = Set(now);
            let result =
                service_snapshot(&txn, service.update(&txn).await.map_err(internal)?).await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    pub async fn rotate_service_key<F>(
        &self,
        owner: UserId,
        id: i64,
        encrypt: F,
    ) -> Result<MerchantOwnedService, MerchantPoolError>
    where
        F: FnOnce(
                ChannelId,
                CredentialId,
            ) -> Result<EncryptedCredentialEnvelope, MerchantPoolError>
            + Send,
    {
        self.bounded(async {
            let txn=self.pool.connection().begin().await.map_err(internal)?;
            lock_owner(&txn,owner).await?;
            let merchant=require_merchant(&txn,owner,true).await?;
            let service=lock_service(&txn,id).await?;
            if service.merchant_id!=merchant.id { return Err(MerchantPoolError::NotFound); }
            if !matches!(service.status.as_str(),"published"|"paused") { return Err(MerchantPoolError::Conflict); }
            let revision=load_revision(&txn,service.published_revision_id.ok_or(MerchantPoolError::Conflict)?,service.id).await?;
            let channel_id=ChannelId::new(revision.channel_id).map_err(internal)?;
            let credential_id=CredentialId::new(revision.credential_id).map_err(internal)?;
            let envelope=encrypt(channel_id,credential_id)?;
            let encrypted=EncryptedJson::from_envelope(serde_json::json!({"version":1,"algorithm":"xchacha20poly1305",
                "key_id":envelope.key_id(),"nonce":URL_SAFE_NO_PAD.encode(envelope.nonce()),"ciphertext":URL_SAFE_NO_PAD.encode(envelope.ciphertext())})).map_err(internal)?;
            let now=DatabaseTimestamp::now_utc();
            let affected=credentials::Entity::update_many().filter(credentials::Column::Id.eq(credential_id.get()))
                .filter(credentials::Column::ChannelId.eq(channel_id.get())).filter(credentials::Column::DeletedAt.is_null())
                .col_expr(credentials::Column::Secret,Expr::value(encrypted)).col_expr(credentials::Column::Status,Expr::value(Status::Enabled.code()))
                .col_expr(credentials::Column::Schedulable,Expr::value(true))
                .col_expr(credentials::Column::RateLimitedAt,Expr::value(None::<DatabaseTimestamp>))
                .col_expr(credentials::Column::RateLimitResetAt,Expr::value(None::<DatabaseTimestamp>))
                .col_expr(credentials::Column::OverloadUntil,Expr::value(None::<DatabaseTimestamp>))
                .col_expr(credentials::Column::TempUnschedulableUntil,Expr::value(None::<DatabaseTimestamp>))
                .col_expr(credentials::Column::TempUnschedulableReason,Expr::value(None::<String>))
                .col_expr(credentials::Column::UpdatedAt,Expr::value(now))
                .exec(&txn).with_subscriber(NoSubscriber::default()).await.map_err(internal)?;
            if affected.rows_affected!=1 { return Err(MerchantPoolError::Conflict); }
            set_channel_enabled(&txn,channel_id.get(),!service.merchant_paused,now).await?;
            enqueue_scheduler_catalog_change(&txn,SchedulerCatalogSubject::Channel(channel_id),now).await.map_err(internal)?;
            audit_control(&txn,owner,"service.key.rotate",id,None,Some(serde_json::json!({"revision_id":revision.id}))).await?;
            let result=service_snapshot(&txn,service).await?;
            txn.commit().await.map_err(internal)?;Ok(result)
        }).await
    }

    /// Restore the existing approved version without publishing or discarding any pending edit.
    pub async fn resume_service(
        &self,
        actor: UserId,
        id: i64,
    ) -> Result<MerchantOwnedService, MerchantPoolError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let initial = find_service(&txn, id).await?;
            let merchant = profiles::Entity::find_by_id(initial.merchant_id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            lock_owner(&txn, UserId::new(merchant.owner_user_id).map_err(internal)?).await?;
            let merchant = profiles::Entity::find_by_id(initial.merchant_id)
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::NotFound)?;
            if merchant.status != "approved" {
                return Err(MerchantPoolError::Conflict);
            }
            let service = lock_service(&txn, id).await?;
            if service.status != "suspended" || !service.platform_suspended {
                return Err(MerchantPoolError::Conflict);
            }
            let revision = load_revision(
                &txn,
                service
                    .published_revision_id
                    .ok_or(MerchantPoolError::Conflict)?,
                id,
            )
            .await?;
            if revision.status != "published" {
                return Err(MerchantPoolError::Conflict);
            }
            let pool = find_pool(&txn, service.pool_id).await?;
            if !pool.enabled {
                return Err(MerchantPoolError::Conflict);
            }
            let models = revision_models(&revision)?;
            validate_ratios(&pool, revision.default_ratio_micros, &models)?;
            for model in &models {
                if super::catalog::model_price(
                    &txn,
                    &model.model,
                    model.ratio_micros.unwrap_or(revision.default_ratio_micros),
                )
                .await?
                .is_none()
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
            }
            let channel = channels::Entity::find_by_id(revision.channel_id)
                .filter(channels::Column::DeletedAt.is_null())
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(MerchantPoolError::Conflict)?;
            if credentials::Entity::find_by_id(revision.credential_id)
                .filter(credentials::Column::ChannelId.eq(channel.id))
                .filter(credentials::Column::DeletedAt.is_null())
                .one(&txn)
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(MerchantPoolError::Conflict);
            }
            let now = DatabaseTimestamp::now_utc();
            let routing = ValidatedChannelRouting::new(
                models.into_iter().map(|model| model.model).collect(),
                vec![GroupId::new(pool.group_id).map_err(internal)?],
            )
            .map_err(internal)?;
            synchronize_channel_routing(
                &txn,
                ChannelId::new(channel.id).map_err(internal)?,
                &routing,
                ChannelAbilityMetadata {
                    enabled: !service.merchant_paused,
                    priority: 0,
                    weight: 1,
                    tag: Some("merchant-pool"),
                },
                now,
            )
            .await
            .map_err(internal)?;
            set_channel_enabled(&txn, channel.id, !service.merchant_paused, now).await?;
            let merchant_paused = service.merchant_paused;
            let mut active = service.into_active_model();
            active.platform_suspended = Set(false);
            active.status = Set(if merchant_paused {
                "paused"
            } else {
                "published"
            }
            .to_owned());
            active.updated_at = Set(now);
            let result =
                service_snapshot(&txn, active.update(&txn).await.map_err(internal)?).await?;
            audit_control(
                &txn,
                actor,
                "service.resume",
                id,
                None,
                Some(serde_json::json!({"published_revision_id":revision.id})),
            )
            .await?;
            txn.commit().await.map_err(internal)?;
            Ok(result)
        })
        .await
    }

    pub async fn review_service(
        &self,
        actor: UserId,
        id: i64,
        review: MerchantReview,
    ) -> Result<MerchantOwnedService, MerchantPoolError> {
        validate_review(&review, true)?;
        self.bounded(async {
            let txn=self.pool.connection().begin().await.map_err(internal)?;
            let initial=find_service(&txn,id).await?;
            let merchant=profiles::Entity::find_by_id(initial.merchant_id).one(&txn).await.map_err(internal)?.ok_or(MerchantPoolError::NotFound)?;
            lock_owner(&txn,UserId::new(merchant.owner_user_id).map_err(internal)?).await?;
            let merchant=profiles::Entity::find_by_id(initial.merchant_id).one(&txn).await.map_err(internal)?.ok_or(MerchantPoolError::NotFound)?;
            let service=lock_service(&txn,id).await?;
            let now=DatabaseTimestamp::now_utc();
            if review.decision=="suspend" {
                if let Some(id)=service.published_revision_id { let revision=load_revision(&txn,id,service.id).await?;set_channel_enabled(&txn,revision.channel_id,false,now).await?; }
                let mut service=service.into_active_model();service.status=Set("suspended".to_owned());service.platform_suspended=Set(true);service.updated_at=Set(now);
                let result=service_snapshot(&txn,service.update(&txn).await.map_err(internal)?).await?;
                audit_control(&txn,actor,"service.suspend",id,None,Some(serde_json::json!({"reason":review.reason}))).await?;
                txn.commit().await.map_err(internal)?;return Ok(result);
            }
            if merchant.status!="approved" { return Err(MerchantPoolError::Conflict); }
            let revision_id=service.pending_revision_id.or(service.published_revision_id).ok_or(MerchantPoolError::Conflict)?;
            let revision=load_revision(&txn,revision_id,id).await?;
            if Some(revision.revision)!=review.expected_revision { return Err(MerchantPoolError::Conflict); }
            if revision.status!="pending" && !(service.status=="suspended" && revision.status=="published" && review.decision=="approve") { return Err(MerchantPoolError::Conflict); }
            let rejected=review.decision=="reject";
            if !rejected {
                let pool=find_pool(&txn,service.pool_id).await?;
                if !pool.enabled { return Err(MerchantPoolError::Conflict); }
                let models=revision_models(&revision)?;
                validate_ratios(&pool,revision.default_ratio_micros,&models)?;
                for model in &models {
                    if super::catalog::model_price(&txn,&model.model,model.ratio_micros.unwrap_or(revision.default_ratio_micros)).await?.is_none() { return Err(MerchantPoolError::InvalidInput); }
                }
                let routing=ValidatedChannelRouting::new(models.into_iter().map(|m|m.model).collect(),vec![GroupId::new(pool.group_id).map_err(internal)?]).map_err(internal)?;
                let channel=ChannelId::new(revision.channel_id).map_err(internal)?;
                synchronize_channel_routing(&txn,channel,&routing,ChannelAbilityMetadata{enabled:!service.merchant_paused,priority:0,weight:1,tag:Some("merchant-pool")},now).await.map_err(internal)?;
                set_channel_enabled(&txn,revision.channel_id,!service.merchant_paused,now).await?;
                if let Some(previous_id)=service.published_revision_id
                    && previous_id!=revision.id {
                        let previous=load_revision(&txn,previous_id,id).await?;
                        set_channel_enabled(&txn,previous.channel_id,false,now).await?;
                        let mut previous=previous.into_active_model();previous.status=Set("superseded".to_owned());previous.updated_at=Set(now);previous.update(&txn).await.map_err(internal)?;
                }
            }
            let mut revision_active=revision.into_active_model();revision_active.status=Set(if rejected{"rejected"}else{"published"}.to_owned());
            revision_active.review_reason=Set(Some(review.reason));revision_active.updated_at=Set(now);
            let revision=revision_active.update(&txn).await.map_err(internal)?;
            let was_published=service.published_revision_id.is_some();
            let merchant_paused=service.merchant_paused;
            let mut active=service.into_active_model();active.updated_at=Set(now);
            if !rejected { active.published_revision_id=Set(Some(revision.id));active.pending_revision_id=Set(None);active.platform_suspended=Set(false);active.status=Set(if merchant_paused{"paused"}else{"published"}.to_owned());active.name=Set(revision.name);active.description=Set(revision.description); }
            else if !was_published { active.status=Set("rejected".to_owned()); }
            let result=service_snapshot(&txn,active.update(&txn).await.map_err(internal)?).await?;
            audit_control(&txn,actor,"service.review",id,None,Some(serde_json::json!({"revision":review.expected_revision,"decision":review.decision,"reason":revision.review_reason}))).await?;
            txn.commit().await.map_err(internal)?;Ok(result)
        }).await
    }
}

fn validate_review(review: &MerchantReview, versioned: bool) -> Result<(), MerchantPoolError> {
    if !matches!(review.decision.as_str(), "approve" | "reject" | "suspend")
        || !af_domain::validate_merchant_text(&review.reason, 2048, false)
        || (versioned
            && review.decision != "suspend"
            && review.expected_revision.is_none_or(|n| n <= 0))
    {
        return Err(MerchantPoolError::InvalidInput);
    }
    Ok(())
}
fn validate_ratios(
    pool: &pools::Model,
    default: i64,
    models: &[MerchantModelWrite],
) -> Result<(), MerchantPoolError> {
    let allowed = pool.min_ratio_micros..=pool.max_ratio_micros;
    if !allowed.contains(&default)
        || models
            .iter()
            .any(|model| !allowed.contains(&model.ratio_micros.unwrap_or(default)))
        || models.iter().any(|model| model.supports_vision)
    {
        return Err(MerchantPoolError::InvalidInput);
    }
    Ok(())
}
async fn set_channel_enabled(
    txn: &DatabaseTransaction,
    id: i64,
    enabled: bool,
    now: DatabaseTimestamp,
) -> Result<(), MerchantPoolError> {
    let channel_id = ChannelId::new(id).map_err(internal)?;
    channels::Entity::update_many()
        .filter(channels::Column::Id.eq(id))
        .filter(channels::Column::DeletedAt.is_null())
        .col_expr(
            channels::Column::Status,
            Expr::value(if enabled {
                Status::Enabled.code()
            } else {
                Status::Disabled.code()
            }),
        )
        .col_expr(channels::Column::UpdatedAt, Expr::value(now))
        .exec(txn)
        .await
        .map_err(internal)?;
    set_channel_abilities_enabled(txn, channel_id, enabled, now)
        .await
        .map_err(internal)?;
    enqueue_scheduler_catalog_change(txn, SchedulerCatalogSubject::Channel(channel_id), now)
        .await
        .map_err(internal)?;
    Ok(())
}
