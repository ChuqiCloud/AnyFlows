use super::*;
use af_db::QuotaRepositoryError;
use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
use af_plugin_contract::{PluginTaskFile, PluginTaskResult, PluginTaskState};

const MAX_PLUGIN_TASK_STATUS_ATTEMPTS: u64 = 8;
const MAX_PLUGIN_TASK_CANCEL_ATTEMPTS: u64 = 8;
const MAX_PLUGIN_TASK_RESULT_ATTEMPTS: u64 = 8;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PluginNativeTaskProtocol {
    Responses,
    Video,
    Image,
}

impl PluginNativeTaskProtocol {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Responses => "openai_responses",
            Self::Video => "openai_video",
            Self::Image => "openai_image",
        }
    }
}

pub(super) struct NativeTaskCache {
    loaded_at: Instant,
    deployments: Arc<Vec<PluginDeploymentRecord>>,
}

fn task_reservation(id: &str) -> Result<BillingReservationId, AdminPluginLifecycleError> {
    BillingReservationId::from_persistence_key(id).map_err(|_| AdminPluginLifecycleError::Internal)
}

fn task_principal(
    record: &PluginHostRecord,
) -> Result<GatewayPrincipal, AdminPluginLifecycleError> {
    let token_id = record.payload["tokenId"]
        .as_i64()
        .and_then(|value| TokenId::new(value).ok())
        .ok_or(AdminPluginLifecycleError::Internal)?;
    let user_id = UserId::new(record.owner_id).map_err(|_| AdminPluginLifecycleError::Internal)?;
    let group_id = record.payload["groupId"]
        .as_i64()
        .and_then(|value| GroupId::new(value).ok())
        .ok_or(AdminPluginLifecycleError::Internal)?;
    Ok(GatewayPrincipal::new(token_id, user_id, group_id))
}

fn next_task_retry_delay(attempt: u64) -> i64 {
    let exponent = attempt.saturating_sub(1).min(4);
    30_i64 * (1_i64 << exponent)
}

fn task_retry_requires_reconciliation(attempt: u64, max_attempts: u64) -> bool {
    attempt >= max_attempts
}

fn task_status_reconciliation_code(error: AdminPluginLifecycleError) -> Option<&'static str> {
    match error {
        AdminPluginLifecycleError::Conflict
        | AdminPluginLifecycleError::Forbidden
        | AdminPluginLifecycleError::NotFound => Some("task_status_requires_reconciliation"),
        _ => None,
    }
}

fn quota_reservation_rejection_code(error: QuotaRepositoryError) -> Option<&'static str> {
    match error {
        QuotaRepositoryError::OutcomeUnknown | QuotaRepositoryError::Query => None,
        QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::MerchantFundingInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. } => Some("quota_insufficient"),
        QuotaRepositoryError::MerchantUsageExceedsAuthorization => {
            Some("quota_reservation_rejected")
        }
        QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::InvalidConfiguration
        | QuotaRepositoryError::NotFound
        | QuotaRepositoryError::Conflict
        | QuotaRepositoryError::Invariant
        | QuotaRepositoryError::ExtensionUnavailable => Some("quota_reservation_unavailable"),
        _ => None,
    }
}

fn task_view(record: &PluginHostRecord) -> Value {
    json!({"id":record.id,"pluginId":record.plugin_id,"model":record.payload["model"],"state":record.state,
        "progressBasisPoints":record.payload["progress"],"output":record.payload["output"],
        "usage":record.payload["usage"],"report":record.payload["report"],"failureCode":record.payload["failureCode"],
        "files":record.payload.get("files").and_then(Value::as_array).map(|files| files.iter().enumerate().map(|(i,file)| json!({"id":i,"name":file["name"],"mediaType":file["mediaType"],"sha256":file["sha256"]})).collect::<Vec<_>>()),
        "settledQuota":record.payload["settledQuota"],"createdAt":record.payload["createdAt"],"updatedAt":record.updated_at})
}

fn admin_task_view(record: &PluginHostRecord) -> Value {
    let mut task = task_view(record);
    task["maxUnits"] = record.payload["maxUnits"].clone();
    task["reconciliationReason"] = record.payload["reconciliationReason"].clone();
    task["reconciledUnits"] = record.payload["reconciledUnits"].clone();
    task
}

fn clear_task_submission_request(payload: &mut Value) -> Result<(), AdminPluginLifecycleError> {
    let payload = payload
        .as_object_mut()
        .ok_or(AdminPluginLifecycleError::Internal)?;
    payload.remove("requestVault");
    payload.remove("request");
    Ok(())
}

fn verify_task_files(files: &[PluginTaskFile]) -> Result<(), AdminPluginLifecycleError> {
    use base64::Engine as _;
    use sha2::Digest as _;
    for file in files {
        let bytes = BASE64
            .decode(&file.bytes_base64)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if let Some(expected) = &file.sha256 {
            let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
        }
    }
    Ok(())
}

impl DatabaseAdminPluginLifecycleService {
    async fn native_task_deployments(
        &self,
    ) -> Result<Arc<Vec<PluginDeploymentRecord>>, AdminPluginLifecycleError> {
        let mut cache = self.data_plane()?.native_tasks.lock().await;
        if let Some(snapshot) = cache.as_ref()
            && snapshot.loaded_at.elapsed() < Duration::from_secs(2)
        {
            return Ok(Arc::clone(&snapshot.deployments));
        }
        let mut cursor = None;
        let mut deployments = Vec::new();
        loop {
            let page = self
                .repository
                .list_deployments(
                    PluginDeploymentQuery::new(cursor, DEFAULT_PLUGIN_CATALOG_PAGE_SIZE)
                        .map_err(map_catalog_error)?,
                )
                .await
                .map_err(map_catalog_error)?;
            let has_next = page.len() == DEFAULT_PLUGIN_CATALOG_PAGE_SIZE as usize;
            cursor = page.last().map(|deployment| deployment.plugin_id.clone());
            deployments.extend(page.into_iter().filter(|deployment| {
                deployment.enabled
                    && deployment.artifact.approval.is_some()
                    && deployment.artifact.revocation.is_none()
                    && deployment.config["taskPricing"].is_object()
            }));
            if !has_next {
                break;
            }
        }
        let deployments = Arc::new(deployments);
        *cache = Some(NativeTaskCache {
            loaded_at: Instant::now(),
            deployments: Arc::clone(&deployments),
        });
        Ok(deployments)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "网关先按协议选择绑定，再执行该协议的入站校验"
    )]
    pub async fn submit_native_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        protocol: PluginNativeTaskProtocol,
        model: &str,
        validate_request: impl FnOnce() -> Result<Value, AdminPluginLifecycleError> + Send,
        key: &str,
        request_id: &str,
    ) -> Result<Option<Value>, AdminPluginLifecycleError> {
        if self.data_plane.is_none() || self.sidecar_runtime.is_none() {
            return Ok(None);
        }
        let group_id = authentication.principal().group_id().get();
        let deployments = self.native_task_deployments().await?;
        let mut binding = None;
        for deployment in deployments.iter() {
            let tariff = &deployment.config["taskPricing"][model];
            let Some(capability) = tariff["nativeProtocols"][protocol.as_str()].as_str() else {
                continue;
            };
            if !tariff["groupIds"]
                .as_array()
                .is_some_and(|groups| groups.iter().any(|group| group.as_i64() == Some(group_id)))
            {
                continue;
            }
            let version = deployment
                .granted_capabilities
                .iter()
                .find(|grant| grant.id == capability)
                .map(|grant| grant.version)
                .ok_or(AdminPluginLifecycleError::Forbidden)?;
            if binding
                .replace((
                    deployment.plugin_id.clone(),
                    capability.to_owned(),
                    version,
                    deployment.generation,
                ))
                .is_some()
            {
                return Err(AdminPluginLifecycleError::Conflict);
            }
        }
        let Some((plugin, capability, version, generation)) = binding else {
            return Ok(None);
        };
        let request = validate_request()?;
        Ok(Some(
            self.submit_plugin_task(
                authentication,
                &plugin,
                &capability,
                version,
                model,
                request,
                key,
                request_id,
                Some((protocol, generation)),
            )
            .await?,
        ))
    }

    /// Rates are administrator configuration, never accepted from the model caller.
    #[allow(
        clippy::too_many_arguments,
        reason = "模型任务入口需要固定携带认证、插件、能力、模型和幂等上下文"
    )]
    pub async fn submit_model_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        plugin_id: &str,
        capability_id: &str,
        version: u16,
        model: &str,
        request: Value,
        key: &str,
        request_id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        self.submit_plugin_task(
            authentication,
            plugin_id,
            capability_id,
            version,
            model,
            request,
            key,
            request_id,
            None,
        )
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "模型任务提交携带认证、能力、幂等键与协议绑定"
    )]
    async fn submit_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        plugin_id: &str,
        capability_id: &str,
        version: u16,
        model: &str,
        request: Value,
        key: &str,
        request_id: &str,
        native: Option<(PluginNativeTaskProtocol, i64)>,
    ) -> Result<Value, AdminPluginLifecycleError> {
        let principal = authentication.principal();
        if !authentication.model_policy().allows(model)
            || principal.organization_principal().is_some()
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let task = PluginTaskOperation::Submit {
            model: model.into(),
            request: request.clone(),
            idempotency_key: Some(key.into()),
        };
        task.validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let (deployment, _) = self
            .prepare_plugin_operation(plugin_id, capability_id, version, request_id, 1024 * 1024)
            .await?;
        if native.is_some_and(|(_, generation)| generation != deployment.generation) {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let manifest: PluginManifest = serde_json::from_value(deployment.artifact.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let input_schema = manifest
            .capabilities
            .iter()
            .find(|declaration| {
                declaration.id.as_str() == capability_id
                    && declaration.version == version
                    && declaration.access == CapabilityAccess::Write
                    && declaration.side_effect == SideEffectClass::ExternalWrite
                    && declaration
                        .write_semantics
                        .is_some_and(|semantics| semantics.native_idempotency_key)
                    && declaration.output_schema.is_some()
            })
            .and_then(|declaration| declaration.input_schema.as_ref())
            .ok_or(AdminPluginLifecycleError::Forbidden)?;
        let tariff = &deployment.config["taskPricing"][model];
        let groups = tariff["groupIds"]
            .as_array()
            .ok_or(AdminPluginLifecycleError::Forbidden)?;
        if !groups
            .iter()
            .any(|group| group.as_i64() == Some(principal.group_id().get()))
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let rate = tariff["quotaPerUnit"]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let max_units = tariff["maxUnits"]
            .as_u64()
            .filter(|value| *value > 0 && *value <= 1_000_000_000)
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let reserve = rate
            .checked_mul(
                i64::try_from(max_units).map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
            )
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let identity = json!([
            principal.user_id().get(),
            principal.token_id().get(),
            plugin_id,
            key
        ]);
        let id = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&identity).map_err(|_| AdminPluginLifecycleError::Internal)?
            )
        )[..32]
            .to_owned();
        let operation_payload = serde_json::to_value(PluginTaskOperation::Submit {
            model: model.into(),
            request: request.clone(),
            idempotency_key: Some(id.clone()),
        })
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        // Screen the exact task sent to the sidecar before reserving quota.
        validate_public_plugin_configuration(input_schema, &operation_payload)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if serde_json::to_vec(&operation_payload)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
            .len()
            > manifest.limits.max_response_bytes as usize
        {
            return Err(AdminPluginLifecycleError::PayloadTooLarge);
        }
        let mut operation = json!([capability_id, version, model, request]);
        let native_protocol = native.map(|(protocol, _)| protocol.as_str());
        if let Some(protocol) = native_protocol {
            operation
                .as_array_mut()
                .expect("operation identity is an array")
                .push(json!(protocol));
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&operation).map_err(|_| AdminPluginLifecycleError::Internal)?
            )
        );
        let now = current_timestamp()?;
        let request_aad = format!("plugin-task:{id}:request");
        let request_vault = self.seal(&request_aad, &request)?;
        let record = self.data_plane()?.records.insert(PluginHostRecord {
            kind:"task".into(),id,plugin_id:plugin_id.into(),owner_id:principal.user_id().get(),revision:1,state:"reserving".into(),due_at:Some(now),updated_at:now,
            payload:json!({"model":model,"inputDigest":digest,"requestVault":request_vault,"key":key,"capability":capability_id,"version":version,"nativeProtocol":native_protocol,"createdAt":now,
                "generation":deployment.generation,"tokenId":principal.token_id().get(),"groupId":principal.group_id().get(),
                "quotaPerUnit":rate,"maxUnits":max_units,"reserve":reserve,"submissionAttempts":0}),
        }).await.map_err(persistence_error)?;
        if record.plugin_id != plugin_id || record.payload["inputDigest"] != digest {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let record = if record.state == "reserving" {
            self.start_plugin_task(record, principal, request_id)
                .await?
        } else {
            record
        };
        Ok(task_view(&record))
    }

    async fn start_plugin_task(
        &self,
        mut record: PluginHostRecord,
        principal: GatewayPrincipal,
        request_id: &str,
    ) -> Result<PluginHostRecord, AdminPluginLifecycleError> {
        let plane = self.data_plane()?;
        let now = current_timestamp()?;
        if record.state != "reserving" {
            return Ok(record);
        }
        if record.due_at.is_some_and(|due_at| due_at > now) {
            return Ok(record);
        }
        record.due_at = Some(now + 60);
        record.updated_at = now;
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let reserve = record.payload["reserve"]
            .as_i64()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        if let Err(error) = plane
            .quota
            .reserve_batch_task(
                task_reservation(&record.id)?,
                principal,
                Quota::new(reserve).map_err(|_| AdminPluginLifecycleError::Internal)?,
            )
            .await
        {
            if let Some(failure_code) = quota_reservation_rejection_code(error) {
                record.state = "failed".into();
                record.due_at = None;
                record.payload["failureCode"] = json!(failure_code);
                clear_task_submission_request(&mut record.payload)?;
                record.updated_at = current_timestamp()?;
                record = plane
                    .records
                    .replace(&record)
                    .await
                    .map_err(persistence_error)?;
                return Ok(record);
            }
            return Err(AdminPluginLifecycleError::Conflict);
        }
        record.state = "submitting".into();
        record.due_at = Some(current_timestamp()? + 60);
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let request = if let Some(request_vault) = record.payload.get("requestVault") {
            match self.unseal(&format!("plugin-task:{}:request", record.id), request_vault) {
                Ok(request) => request,
                Err(_) => {
                    return self
                        .release_unsubmitted_plugin_task(record, "task_request_unavailable")
                        .await;
                }
            }
        } else {
            let Some(request) = record.payload.get("request").cloned() else {
                return self
                    .release_unsubmitted_plugin_task(record, "task_request_unavailable")
                    .await;
            };
            request
        };
        let operation = PluginTaskOperation::Submit {
            model: record.payload["model"]
                .as_str()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .into(),
            request,
            idempotency_key: Some(record.id.clone()),
        };
        let result = self.invoke_bound_task(&record, operation, request_id).await;
        match result {
            Ok(PluginTaskResult::Submitted {
                task_id,
                state,
                progress_basis_points,
            }) => {
                record.payload["providerTaskId"] = json!(task_id);
                record.payload["progress"] = json!(progress_basis_points);
                record.state = match state {
                    PluginTaskState::Queued => "queued",
                    PluginTaskState::Running => "running",
                    _ => "submitted",
                }
                .into();
                record.due_at = Some(current_timestamp()? + 5);
                clear_task_submission_request(&mut record.payload)?;
            }
            Err(
                AdminPluginLifecycleError::Conflict
                | AdminPluginLifecycleError::Forbidden
                | AdminPluginLifecycleError::NotFound,
            ) => {
                record.state = "settling".into();
                record.due_at = Some(current_timestamp()? + 5);
                record.payload["terminalState"] = json!("failed");
                record.payload["actualQuota"] = json!(0);
                record.payload["failureCode"] = json!("task_submission_rejected");
                clear_task_submission_request(&mut record.payload)?;
            }
            _ => {
                record.state = "unknown".into();
                record.due_at = Some(current_timestamp()? + 30);
                record.payload["failureCode"] = json!("submission_outcome_unknown");
            }
        }
        record.updated_at = current_timestamp()?;
        let record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        if record.state == "settling" {
            self.settle_plugin_task(record).await
        } else {
            Ok(record)
        }
    }

    async fn release_unsubmitted_plugin_task(
        &self,
        mut record: PluginHostRecord,
        failure_code: &str,
    ) -> Result<PluginHostRecord, AdminPluginLifecycleError> {
        record.state = "settling".into();
        record.due_at = Some(current_timestamp()? + 5);
        record.payload["terminalState"] = json!("failed");
        record.payload["actualQuota"] = json!(0);
        record.payload["failureCode"] = json!(failure_code);
        clear_task_submission_request(&mut record.payload)?;
        record.updated_at = current_timestamp()?;
        let record = self
            .data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        self.settle_plugin_task(record).await
    }

    async fn invoke_bound_task(
        &self,
        record: &PluginHostRecord,
        task: PluginTaskOperation,
        request_id: &str,
    ) -> Result<PluginTaskResult, AdminPluginLifecycleError> {
        let capability = record.payload["capability"]
            .as_str()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let version = record.payload["version"]
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let (deployment, invocation) = self
            .prepare_plugin_operation(
                &record.plugin_id,
                capability,
                version,
                request_id,
                1024 * 1024,
            )
            .await?;
        if record.payload["generation"].as_i64() != Some(deployment.generation) {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        self.sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?
            .invoke_task(
                &self.repository,
                &record.plugin_id,
                deployment.generation,
                &invocation,
                capability,
                version,
                task,
            )
            .await
    }

    async fn owned_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        id: &str,
    ) -> Result<PluginHostRecord, AdminPluginLifecycleError> {
        let record = self
            .data_plane()?
            .records
            .get("task", id)
            .await
            .map_err(persistence_error)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        let principal = authentication.principal();
        if record.owner_id != principal.user_id().get()
            || record.payload["tokenId"].as_i64() != Some(principal.token_id().get())
            || record.payload["groupId"].as_i64() != Some(principal.group_id().get())
            || !record.payload["model"]
                .as_str()
                .is_some_and(|model| authentication.model_policy().allows(model))
        {
            return Err(AdminPluginLifecycleError::NotFound);
        }
        Ok(record)
    }

    pub async fn get_model_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        Ok(task_view(
            &self.owned_plugin_task(authentication, id).await?,
        ))
    }

    pub async fn list_unknown_plugin_tasks(
        &self,
        principal: SessionPrincipal,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<Value, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        if limit == 0
            || limit > 100
            || cursor.is_some_and(|cursor| {
                cursor.len() != 32
                    || !cursor
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let records = self
            .data_plane()?
            .records
            .list("task", None, cursor, None, u64::from(limit))
            .await
            .map_err(persistence_error)?;
        let next_cursor = (records.len() == usize::from(limit))
            .then(|| records.last().map(|record| record.id.clone()))
            .flatten();
        let tasks = records
            .iter()
            .filter(|record| record.state == "unknown")
            .map(admin_task_view)
            .collect::<Vec<_>>();
        Ok(json!({"tasks":tasks,"nextCursor":next_cursor}))
    }

    /// Resolve an unknown submission after an administrator checks the provider-side outcome.
    pub async fn reconcile_unknown_plugin_task(
        &self,
        principal: SessionPrincipal,
        id: &str,
        actual_units: u64,
        reason: &str,
        request_id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/tasks/{taskId}/reconcile",
            "reconcile_unknown_plugin_task",
            "plugin_task",
            Some(id),
        )
        .await?;
        let result = self
            .reconcile_unknown_plugin_task_authorized(id, actual_units, reason, principal)
            .await;
        let after = result
            .as_ref()
            .ok()
            .map(|(_, audit_info)| audit_info.clone());
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/tasks/{taskId}/reconcile",
            "reconcile_unknown_plugin_task",
            "plugin_task",
            Some(id.to_owned()),
            None,
            after,
            result.map(|(task, _)| task),
        )
        .await
    }

    async fn reconcile_unknown_plugin_task_authorized(
        &self,
        id: &str,
        actual_units: u64,
        reason: &str,
        principal: SessionPrincipal,
    ) -> Result<(Value, Value), AdminPluginLifecycleError> {
        if reason.is_empty()
            || reason.len() > 256
            || reason.trim() != reason
            || reason.chars().any(char::is_control)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let plane = self.data_plane()?;
        let mut record = plane
            .records
            .get("task", id)
            .await
            .map_err(persistence_error)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if record.state != "unknown" {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let max_units = record.payload["maxUnits"]
            .as_u64()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        if actual_units > max_units {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let rate = record.payload["quotaPerUnit"]
            .as_i64()
            .filter(|value| *value > 0)
            .ok_or(AdminPluginLifecycleError::Internal)?;
        let actual_quota = rate
            .checked_mul(
                i64::try_from(actual_units).map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
            )
            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
        let audit_info = json!({
            "actualUnits": actual_units,
            "settledQuota": actual_quota,
            "reason": reason
        });
        record.state = "settling".into();
        record.due_at = Some(current_timestamp()? + 60);
        record.payload["terminalState"] = json!("failed");
        record.payload["actualQuota"] = json!(actual_quota);
        record.payload["failureCode"] = json!("manual_reconciliation");
        record.payload["cancelRequested"] = json!(false);
        record.payload["reconciledUnits"] = json!(actual_units);
        record.payload["reconciliationReason"] = json!(reason);
        record.payload["reconciledBy"] = json!(principal.user_id().get());
        clear_task_submission_request(&mut record.payload)?;
        record.updated_at = current_timestamp()?;
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let record = match self.settle_plugin_task(record.clone()).await {
            Ok(record) => record,
            Err(AdminPluginLifecycleError::Conflict) => {
                let current = plane
                    .records
                    .get("task", id)
                    .await
                    .map_err(persistence_error)?
                    .ok_or(AdminPluginLifecycleError::NotFound)?;
                if current.state == "failed"
                    && current.payload["failureCode"] == "manual_reconciliation"
                {
                    current
                } else {
                    return Err(AdminPluginLifecycleError::Conflict);
                }
            }
            Err(error) => return Err(error),
        };
        Ok((admin_task_view(&record), audit_info))
    }

    pub async fn get_native_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        protocol: PluginNativeTaskProtocol,
        id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        let record = self.owned_plugin_task(authentication, id).await?;
        if record.payload["nativeProtocol"].as_str() != Some(protocol.as_str()) {
            return Err(AdminPluginLifecycleError::NotFound);
        }
        Ok(task_view(&record))
    }

    pub async fn get_model_plugin_task_file(
        &self,
        authentication: &crate::TokenAuthentication,
        id: &str,
        index: usize,
    ) -> Result<Vec<u8>, AdminPluginLifecycleError> {
        let record = self.owned_plugin_task(authentication, id).await?;
        if record.state != "succeeded" {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        let file = record.payload["files"]
            .as_array()
            .and_then(|files| files.get(index))
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        BASE64
            .decode(
                file["bytesBase64"]
                    .as_str()
                    .ok_or(AdminPluginLifecycleError::Internal)?,
            )
            .map_err(|_| AdminPluginLifecycleError::Internal)
    }

    pub async fn cancel_model_plugin_task(
        &self,
        authentication: &crate::TokenAuthentication,
        id: &str,
        request_id: &str,
    ) -> Result<Value, AdminPluginLifecycleError> {
        let mut record = self.owned_plugin_task(authentication, id).await?;
        if record.payload["cancelRequested"] == true {
            return Ok(task_view(&record));
        }
        if !matches!(record.state.as_str(), "submitted" | "queued" | "running") {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        record.payload["cancelRequested"] = json!(true);
        record.payload["cancelAttempts"] = json!(0);
        record.state = "polling".into();
        record.due_at = Some(current_timestamp()? + 60);
        record = self
            .data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let record = self.dispatch_plugin_task_cancel(record, request_id).await?;
        Ok(task_view(&record))
    }

    async fn dispatch_plugin_task_cancel(
        &self,
        mut record: PluginHostRecord,
        request_id: &str,
    ) -> Result<PluginHostRecord, AdminPluginLifecycleError> {
        let task_id = record.payload["providerTaskId"]
            .as_str()
            .ok_or(AdminPluginLifecycleError::Internal)?
            .to_owned();
        match self
            .invoke_bound_task(
                &record,
                PluginTaskOperation::Cancel {
                    task_id,
                    reason: None,
                },
                request_id,
            )
            .await
        {
            Ok(_) => {
                record.payload["cancelRequested"] = json!(false);
                record.payload["cancelAttempts"] = json!(0);
                record.payload["failureCode"] = Value::Null;
                record.state = "running".into();
                record.due_at = Some(current_timestamp()? + 5);
            }
            Err(AdminPluginLifecycleError::Conflict) => {
                record.payload["cancelRequested"] = json!(false);
                record.payload["cancelAttempts"] = json!(0);
                record.payload["failureCode"] = json!("task_cancel_not_applied");
                record.state = "running".into();
                record.due_at = Some(current_timestamp()? + 5);
            }
            Err(AdminPluginLifecycleError::Forbidden | AdminPluginLifecycleError::NotFound) => {
                let task_id = record.id.clone();
                self.park_task_for_reconciliation(record, "task_cancel_requires_reconciliation")
                    .await?;
                return self
                    .data_plane()?
                    .records
                    .get("task", &task_id)
                    .await
                    .map_err(persistence_error)?
                    .ok_or(AdminPluginLifecycleError::NotFound);
            }
            Err(_) => {
                let attempt = record.payload["cancelAttempts"]
                    .as_u64()
                    .unwrap_or_default()
                    .saturating_add(1);
                if task_retry_requires_reconciliation(attempt, MAX_PLUGIN_TASK_CANCEL_ATTEMPTS) {
                    let task_id = record.id.clone();
                    self.park_task_for_reconciliation(
                        record,
                        "task_cancel_requires_reconciliation",
                    )
                    .await?;
                    return self
                        .data_plane()?
                        .records
                        .get("task", &task_id)
                        .await
                        .map_err(persistence_error)?
                        .ok_or(AdminPluginLifecycleError::NotFound);
                }
                record.payload["cancelAttempts"] = json!(attempt);
                record.payload["failureCode"] = json!("task_cancel_temporarily_unavailable");
                record.state = "polling".into();
                record.due_at = Some(current_timestamp()? + next_task_retry_delay(attempt));
            }
        }
        record.updated_at = current_timestamp()?;
        self.data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)
    }

    async fn settle_plugin_task(
        &self,
        mut record: PluginHostRecord,
    ) -> Result<PluginHostRecord, AdminPluginLifecycleError> {
        let plane = self.data_plane()?;
        let actual = record.payload["actualQuota"]
            .as_i64()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        plane
            .quota
            .settle_batch_task(
                task_reservation(&record.id)?,
                Quota::new(actual).map_err(|_| AdminPluginLifecycleError::Internal)?,
            )
            .await
            .map_err(|_| AdminPluginLifecycleError::Conflict)?;
        record.state = record.payload["terminalState"]
            .as_str()
            .ok_or(AdminPluginLifecycleError::Internal)?
            .into();
        record.payload["settledQuota"] = json!(actual);
        record.due_at = None;
        record.updated_at = current_timestamp()?;
        plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)
    }

    async fn poll_plugin_task(
        &self,
        mut record: PluginHostRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        let plane = self.data_plane()?;
        if record.state == "settling" {
            self.settle_plugin_task(record).await?;
            return Ok(());
        }
        if record.state == "submitting" {
            record.state = "unknown".into();
            record.due_at = Some(current_timestamp()? + 30);
            record.payload["failureCode"] = json!("submission_outcome_unknown");
            plane
                .records
                .replace(&record)
                .await
                .map_err(persistence_error)?;
            return Ok(());
        }
        if record.state == "unknown" {
            return self.retry_unknown_plugin_task(record).await;
        }
        if record.payload["cancelRequested"] == true {
            self.dispatch_plugin_task_cancel(record, "plugin-task-cancel-recovery")
                .await?;
            return Ok(());
        }
        record.state = "polling".into();
        record.due_at = Some(current_timestamp()? + 60);
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let task_id = record.payload["providerTaskId"]
            .as_str()
            .ok_or(AdminPluginLifecycleError::Internal)?
            .to_owned();
        let status = self
            .invoke_bound_task(
                &record,
                PluginTaskOperation::Status {
                    task_id: task_id.clone(),
                },
                "plugin-task-poll",
            )
            .await;
        let (state, progress_basis_points, failure_code) = match status {
            Ok(PluginTaskResult::Status {
                state,
                progress_basis_points,
                failure_code,
                ..
            }) => (state, progress_basis_points, failure_code),
            Ok(_) => {
                return self
                    .park_task_for_reconciliation(record, "task_status_requires_reconciliation")
                    .await;
            }
            Err(error) => {
                if let Some(failure_code) = task_status_reconciliation_code(error) {
                    return self
                        .park_task_for_reconciliation(record, failure_code)
                        .await;
                }
                let attempt = record.payload["statusAttempts"]
                    .as_u64()
                    .unwrap_or_default()
                    .saturating_add(1);
                if task_retry_requires_reconciliation(attempt, MAX_PLUGIN_TASK_STATUS_ATTEMPTS) {
                    self.park_task_for_reconciliation(
                        record,
                        "task_status_requires_reconciliation",
                    )
                    .await?;
                    return Ok(());
                }
                record.state = "running".into();
                record.due_at = Some(current_timestamp()? + next_task_retry_delay(attempt));
                record.payload["statusAttempts"] = json!(attempt);
                record.payload["failureCode"] = json!("task_status_temporarily_unavailable");
                record.updated_at = current_timestamp()?;
                plane
                    .records
                    .replace(&record)
                    .await
                    .map_err(persistence_error)?;
                return Ok(());
            }
        };
        record.payload["statusAttempts"] = json!(0);
        let mut terminal = None;
        record.payload["progress"] = json!(progress_basis_points);
        record.payload["failureCode"] = json!(failure_code);
        match state {
            PluginTaskState::Succeeded => {
                match self
                    .invoke_bound_task(
                        &record,
                        PluginTaskOperation::Result {
                            task_id: task_id.clone(),
                            response: json!({"state":"succeeded"}),
                        },
                        "plugin-task-result",
                    )
                    .await
                {
                    Ok(PluginTaskResult::Result {
                        output,
                        files,
                        report,
                        usage,
                        task_id: returned_id,
                    }) => {
                        if returned_id != task_id {
                            return self
                                .park_task_for_reconciliation(
                                    record,
                                    "task_result_requires_reconciliation",
                                )
                                .await;
                        }
                        let Some(usage) = usage else {
                            return self
                                .park_task_for_reconciliation(
                                    record,
                                    "task_result_requires_reconciliation",
                                )
                                .await;
                        };
                        let files_are_valid = verify_task_files(&files).is_ok();
                        let ceiling = record.payload["maxUnits"].as_u64();
                        let rate = record.payload["quotaPerUnit"].as_i64();
                        let units = i64::try_from(usage.units).ok();
                        let actual = rate
                            .zip(units)
                            .and_then(|(rate, units)| rate.checked_mul(units));
                        if !files_are_valid
                            || ceiling.is_none_or(|ceiling| usage.units > ceiling)
                            || actual.is_none()
                        {
                            return self
                                .park_task_for_reconciliation(
                                    record,
                                    "task_result_requires_reconciliation",
                                )
                                .await;
                        }
                        let Some(actual) = actual else {
                            return self
                                .park_task_for_reconciliation(
                                    record,
                                    "task_result_requires_reconciliation",
                                )
                                .await;
                        };
                        record.payload["output"] = output;
                        record.payload["files"] = json!(files);
                        record.payload["report"] = json!(report);
                        record.payload["usage"] = json!(usage);
                        terminal = Some(("succeeded", actual));
                    }
                    Ok(_) => {
                        return self
                            .park_task_for_reconciliation(
                                record,
                                "task_result_requires_reconciliation",
                            )
                            .await;
                    }
                    Err(
                        AdminPluginLifecycleError::Conflict
                        | AdminPluginLifecycleError::Forbidden
                        | AdminPluginLifecycleError::NotFound,
                    ) => {
                        return self
                            .park_task_for_reconciliation(
                                record,
                                "task_result_requires_reconciliation",
                            )
                            .await;
                    }
                    Err(_) => {
                        let attempt = record.payload["resultAttempts"]
                            .as_u64()
                            .unwrap_or_default()
                            .saturating_add(1);
                        if task_retry_requires_reconciliation(
                            attempt,
                            MAX_PLUGIN_TASK_RESULT_ATTEMPTS,
                        ) {
                            return self
                                .park_task_for_reconciliation(
                                    record,
                                    "task_result_requires_reconciliation",
                                )
                                .await;
                        }
                        record.payload["resultAttempts"] = json!(attempt);
                        record.payload["failureCode"] =
                            json!("task_result_temporarily_unavailable");
                        record.state = "running".into();
                        record.due_at = Some(current_timestamp()? + next_task_retry_delay(attempt));
                        record.updated_at = current_timestamp()?;
                        plane
                            .records
                            .replace(&record)
                            .await
                            .map_err(persistence_error)?;
                        return Ok(());
                    }
                }
            }
            PluginTaskState::Failed => terminal = Some(("failed", 0)),
            PluginTaskState::Cancelled => terminal = Some(("cancelled", 0)),
            _ => {}
        }
        record.state = if terminal.is_some() {
            "settling"
        } else {
            "running"
        }
        .into();
        record.due_at = Some(current_timestamp()? + 5);
        record.updated_at = current_timestamp()?;
        if let Some((state, quota)) = terminal {
            record.payload["terminalState"] = json!(state);
            record.payload["actualQuota"] = json!(quota);
        }
        let record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        if record.state == "settling" {
            self.settle_plugin_task(record).await?;
        }
        Ok(())
    }

    async fn retry_unknown_plugin_task(
        &self,
        mut record: PluginHostRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        let request = if let Some(request_vault) = record.payload.get("requestVault") {
            match self.unseal(&format!("plugin-task:{}:request", record.id), request_vault) {
                Ok(request) => request,
                Err(_) => return self.park_unknown_task_for_reconciliation(record).await,
            }
        } else if let Some(request) = record.payload.get("request") {
            request.clone()
        } else {
            return self.park_unknown_task_for_reconciliation(record).await;
        };
        let attempt = record.payload["submissionAttempts"]
            .as_u64()
            .unwrap_or_default()
            .saturating_add(1);
        record.state = "submitting".into();
        record.due_at = Some(current_timestamp()? + 60);
        record.payload["submissionAttempts"] = json!(attempt);
        record.updated_at = current_timestamp()?;
        record = self
            .data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let operation = PluginTaskOperation::Submit {
            model: record.payload["model"]
                .as_str()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .to_owned(),
            request,
            idempotency_key: Some(record.id.clone()),
        };
        match self
            .invoke_bound_task(&record, operation, "plugin-task-submit-recovery")
            .await
        {
            Ok(PluginTaskResult::Submitted {
                task_id,
                state,
                progress_basis_points,
            }) => {
                record.payload["providerTaskId"] = json!(task_id);
                record.payload["progress"] = json!(progress_basis_points);
                record.payload["failureCode"] = Value::Null;
                record.state = match state {
                    PluginTaskState::Queued => "queued",
                    PluginTaskState::Running => "running",
                    _ => "submitted",
                }
                .into();
                record.due_at = Some(current_timestamp()? + 5);
                clear_task_submission_request(&mut record.payload)?;
            }
            Err(
                AdminPluginLifecycleError::Conflict
                | AdminPluginLifecycleError::Forbidden
                | AdminPluginLifecycleError::NotFound,
            ) => {
                record.state = "unknown".into();
                record.due_at = None;
                record.payload["failureCode"] = json!("manual_reconciliation_required");
                clear_task_submission_request(&mut record.payload)?;
            }
            _ => {
                record.state = "unknown".into();
                record.due_at = Some(current_timestamp()? + next_task_retry_delay(attempt));
                record.payload["failureCode"] = json!("submission_outcome_unknown");
            }
        }
        record.updated_at = current_timestamp()?;
        self.data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        Ok(())
    }

    async fn park_unknown_task_for_reconciliation(
        &self,
        record: PluginHostRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        self.park_task_for_reconciliation(record, "manual_reconciliation_required")
            .await
    }

    async fn park_task_for_reconciliation(
        &self,
        mut record: PluginHostRecord,
        failure_code: &str,
    ) -> Result<(), AdminPluginLifecycleError> {
        record.state = "unknown".into();
        record.due_at = None;
        record.payload["failureCode"] = json!(failure_code);
        record.payload["cancelRequested"] = json!(false);
        clear_task_submission_request(&mut record.payload)?;
        record.updated_at = current_timestamp()?;
        self.data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        Ok(())
    }

    pub async fn poll_plugin_tasks(&self) {
        let Ok(plane) = self.data_plane() else {
            return;
        };
        if let Some(storage) = self.storage_dispatcher.as_ref() {
            storage.purge_expired().await;
        }
        let Ok(now) = current_timestamp() else {
            return;
        };
        let Ok(records) = plane.records.list("task", None, None, Some(now), 32).await else {
            return;
        };
        for record in records {
            if record.state == "reserving" {
                if let Ok(principal) = task_principal(&record) {
                    let _ = self
                        .start_plugin_task(record, principal, "plugin-task-submit-recovery")
                        .await;
                }
                continue;
            }
            let _ = self.poll_plugin_task(record).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clear_task_submission_request, next_task_retry_delay, quota_reservation_rejection_code,
        task_retry_requires_reconciliation, task_status_reconciliation_code,
    };
    use crate::plugin_lifecycle::AdminPluginLifecycleError;
    use af_db::QuotaRepositoryError;
    use serde_json::json;

    #[test]
    fn unknown_submission_retries_with_a_bounded_delay() {
        assert_eq!(next_task_retry_delay(0), 30);
        assert_eq!(next_task_retry_delay(1), 30);
        assert_eq!(next_task_retry_delay(2), 60);
        assert_eq!(next_task_retry_delay(3), 120);
        assert_eq!(next_task_retry_delay(4), 240);
        assert_eq!(next_task_retry_delay(5), 480);
        assert_eq!(next_task_retry_delay(100), 480);
    }

    #[test]
    fn transient_task_failures_stop_after_the_retry_limit() {
        assert!(!task_retry_requires_reconciliation(7, 8));
        assert!(task_retry_requires_reconciliation(8, 8));
        assert!(task_retry_requires_reconciliation(u64::MAX, 8));
    }

    #[test]
    fn unavailable_task_state_requires_manual_reconciliation() {
        assert_eq!(
            task_status_reconciliation_code(AdminPluginLifecycleError::Conflict),
            Some("task_status_requires_reconciliation")
        );
        assert_eq!(
            task_status_reconciliation_code(AdminPluginLifecycleError::Forbidden),
            Some("task_status_requires_reconciliation")
        );
        assert_eq!(
            task_status_reconciliation_code(AdminPluginLifecycleError::NotFound),
            Some("task_status_requires_reconciliation")
        );
        assert_eq!(
            task_status_reconciliation_code(AdminPluginLifecycleError::RuntimeUnavailable),
            None
        );
    }

    #[test]
    fn completed_or_reconciled_tasks_clear_legacy_plaintext_and_encrypted_requests() {
        let mut payload = json!({
            "request": {"prompt": "legacy"},
            "requestVault": {"ciphertext": "encrypted"},
            "model": "example-model"
        });

        clear_task_submission_request(&mut payload).unwrap();

        assert!(payload.get("request").is_none());
        assert!(payload.get("requestVault").is_none());
        assert_eq!(payload["model"], "example-model");
    }

    #[test]
    fn definite_quota_rejections_are_not_retried_as_ambiguous_reservations() {
        assert_eq!(
            quota_reservation_rejection_code(QuotaRepositoryError::UserQuotaInsufficient),
            Some("quota_insufficient")
        );
        assert_eq!(
            quota_reservation_rejection_code(QuotaRepositoryError::OutcomeUnknown),
            None
        );
        assert_eq!(
            quota_reservation_rejection_code(QuotaRepositoryError::Query),
            None
        );
        assert_eq!(
            quota_reservation_rejection_code(QuotaRepositoryError::Conflict),
            Some("quota_reservation_unavailable")
        );
        assert_eq!(
            quota_reservation_rejection_code(
                QuotaRepositoryError::MerchantUsageExceedsAuthorization
            ),
            Some("quota_reservation_rejected")
        );
    }

    #[test]
    fn only_admin_task_views_include_reconciliation_details() {
        let record = af_db::PluginHostRecord {
            kind: "task".into(),
            id: "a".repeat(32),
            plugin_id: "org.example.tasks".into(),
            owner_id: 1,
            revision: 1,
            state: "unknown".into(),
            payload: json!({
                "model": "example-model",
                "maxUnits": 4,
                "reconciliationReason": "checked upstream",
                "reconciledUnits": 2
            }),
            due_at: None,
            updated_at: 1,
        };

        let user_view = super::task_view(&record);
        let admin_view = super::admin_task_view(&record);

        assert!(user_view.get("maxUnits").is_none());
        assert!(user_view.get("reconciliationReason").is_none());
        assert_eq!(admin_view["maxUnits"], 4);
        assert_eq!(admin_view["reconciliationReason"], "checked upstream");
        assert_eq!(admin_view["reconciledUnits"], 2);
    }
}
