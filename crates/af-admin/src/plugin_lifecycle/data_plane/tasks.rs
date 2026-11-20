use super::*;
use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use af_plugin_contract::{PluginTaskFile, PluginTaskResult, PluginTaskState};

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

fn task_view(record: &PluginHostRecord) -> Value {
    json!({"id":record.id,"pluginId":record.plugin_id,"model":record.payload["model"],"state":record.state,
        "progressBasisPoints":record.payload["progress"],"output":record.payload["output"],
        "usage":record.payload["usage"],"report":record.payload["report"],"failureCode":record.payload["failureCode"],
        "files":record.payload.get("files").and_then(Value::as_array).map(|files| files.iter().enumerate().map(|(i,file)| json!({"id":i,"name":file["name"],"mediaType":file["mediaType"],"sha256":file["sha256"]})).collect::<Vec<_>>()),
        "settledQuota":record.payload["settledQuota"],"createdAt":record.payload["createdAt"],"updatedAt":record.updated_at})
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
        let record = self.data_plane()?.records.insert(PluginHostRecord {
            kind:"task".into(),id,plugin_id:plugin_id.into(),owner_id:principal.user_id().get(),revision:1,state:"reserving".into(),due_at:None,updated_at:now,
            payload:json!({"model":model,"inputDigest":digest,"request":request,"key":key,"capability":capability_id,"version":version,"nativeProtocol":native_protocol,"createdAt":now,
                "generation":deployment.generation,"tokenId":principal.token_id().get(),"groupId":principal.group_id().get(),
                "quotaPerUnit":rate,"maxUnits":max_units,"reserve":reserve}),
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
        let reserve = record.payload["reserve"]
            .as_i64()
            .ok_or(AdminPluginLifecycleError::Internal)?;
        plane
            .quota
            .reserve_batch_task(
                task_reservation(&record.id)?,
                principal,
                Quota::new(reserve).map_err(|_| AdminPluginLifecycleError::Internal)?,
            )
            .await
            .map_err(|_| AdminPluginLifecycleError::Conflict)?;
        record.state = "submitting".into();
        record.due_at = Some(current_timestamp()? + 60);
        record = plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let operation = PluginTaskOperation::Submit {
            model: record.payload["model"]
                .as_str()
                .ok_or(AdminPluginLifecycleError::Internal)?
                .into(),
            request: record.payload["request"].clone(),
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
            }
            _ => {
                record.state = "unknown".into();
                record.due_at = None;
                record.payload["failureCode"] = json!("submission_outcome_unknown");
            }
        }
        record
            .payload
            .as_object_mut()
            .ok_or(AdminPluginLifecycleError::Internal)?
            .remove("request");
        record.updated_at = current_timestamp()?;
        plane
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)
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
        if !matches!(record.state.as_str(), "submitted" | "queued" | "running") {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        record.state = "polling".into();
        record.due_at = Some(current_timestamp()? + 60);
        record = self
            .data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        let task_id = record.payload["providerTaskId"]
            .as_str()
            .ok_or(AdminPluginLifecycleError::Internal)?
            .into();
        // Confirmation comes from the provider status, not from the cancel request.
        let _ = self
            .invoke_bound_task(
                &record,
                PluginTaskOperation::Cancel {
                    task_id,
                    reason: None,
                },
                request_id,
            )
            .await;
        record.state = "running".into();
        record.due_at = Some(current_timestamp()? + 5);
        let record = self
            .data_plane()?
            .records
            .replace(&record)
            .await
            .map_err(persistence_error)?;
        Ok(task_view(&record))
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
            record.due_at = None;
            record.payload["failureCode"] = json!("submission_outcome_unknown");
            plane
                .records
                .replace(&record)
                .await
                .map_err(persistence_error)?;
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
        let mut terminal = None;
        if let Ok(PluginTaskResult::Status {
            state,
            progress_basis_points,
            failure_code,
            ..
        }) = status
        {
            record.payload["progress"] = json!(progress_basis_points);
            record.payload["failureCode"] = json!(failure_code);
            match state {
                PluginTaskState::Succeeded => {
                    if let Ok(PluginTaskResult::Result {
                        output,
                        files,
                        report,
                        usage,
                        task_id: returned_id,
                    }) = self
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
                        if returned_id != task_id {
                            return Err(AdminPluginLifecycleError::InvalidInput);
                        }
                        let usage = usage.ok_or(AdminPluginLifecycleError::InvalidInput)?;
                        verify_task_files(&files)?;
                        let ceiling = record.payload["maxUnits"]
                            .as_u64()
                            .ok_or(AdminPluginLifecycleError::Internal)?;
                        if usage.units > ceiling {
                            return Err(AdminPluginLifecycleError::InvalidInput);
                        }
                        let rate = record.payload["quotaPerUnit"]
                            .as_i64()
                            .ok_or(AdminPluginLifecycleError::Internal)?;
                        let actual = rate
                            .checked_mul(
                                i64::try_from(usage.units)
                                    .map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
                            )
                            .ok_or(AdminPluginLifecycleError::InvalidInput)?;
                        record.payload["output"] = output;
                        record.payload["files"] = json!(files);
                        record.payload["report"] = json!(report);
                        record.payload["usage"] = json!(usage);
                        terminal = Some(("succeeded", actual));
                    }
                }
                PluginTaskState::Failed => terminal = Some(("failed", 0)),
                PluginTaskState::Cancelled => terminal = Some(("cancelled", 0)),
                _ => {}
            }
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

    pub async fn poll_plugin_tasks(&self) {
        let Ok(plane) = self.data_plane() else {
            return;
        };
        let Ok(now) = current_timestamp() else {
            return;
        };
        let Ok(records) = plane.records.list("task", None, None, Some(now), 32).await else {
            return;
        };
        for mut record in records {
            if record.state == "reserving" {
                // Older rows could be due before the submission had reserved quota.
                record.due_at = None;
                let _ = plane.records.replace(&record).await;
                continue;
            }
            let _ = self.poll_plugin_task(record).await;
        }
    }
}
