use std::sync::Arc;

use af_admin::{
    GroupModelPolicyError, GroupModelPolicyRecord, GroupModelPolicyService,
    GroupModelRatioOverridePolicyRecord, SessionAuthentication, SessionAuthenticator,
};
use af_domain::GroupId;
use axum::{
    Json, Router,
    extract::{Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroupModelPolicy)]
pub(crate) struct AdminGroupModelPolicyResponse {
    #[schema(minimum = 1)]
    group_id: i64,
    /// NULL 表示不限制；非空数组表示仅允许精确匹配列出的模型。
    #[schema(required = true, min_items = 1, max_items = 512)]
    allowed_models: Option<Vec<String>>,
    /// 来源分组到当前目标分组的模型倍率覆盖。
    #[schema(max_items = 1024)]
    ratio_overrides: Vec<AdminGroupModelRatioOverride>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminGroupModelRatioOverride {
    #[schema(minimum = 1)]
    source_group_id: i64,
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(minimum = 0)]
    ratio_micros: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminGroupModelPolicyWriteRequest {
    /// NULL 清除限制；非空数组设置白名单。
    #[serde(deserialize_with = "deserialize_allowed_models")]
    #[schema(required = true, min_items = 1, max_items = 512)]
    allowed_models: Option<Vec<String>>,
    /// 提供时整体替换当前目标分组的模型倍率覆盖；省略时保留已有覆盖。
    #[schema(max_items = 1024)]
    ratio_overrides: Option<Vec<AdminGroupModelRatioOverride>>,
}

fn deserialize_allowed_models<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<String>>::deserialize(deserializer)
}

impl From<GroupModelPolicyRecord> for AdminGroupModelPolicyResponse {
    fn from(policy: GroupModelPolicyRecord) -> Self {
        Self {
            group_id: policy.group_id.get(),
            allowed_models: policy.allowed_models,
            ratio_overrides: policy
                .ratio_overrides
                .into_iter()
                .map(|item| AdminGroupModelRatioOverride {
                    source_group_id: item.source_group_id.get(),
                    model: item.model,
                    ratio_micros: item.ratio_micros,
                })
                .collect(),
        }
    }
}

/// 构建分组模型白名单管理 API。
pub fn build_group_model_policy_router(
    service: Arc<dyn GroupModelPolicyService>,
    authenticator: Arc<dyn SessionAuthenticator>,
    config: &af_config::ServerConfig,
) -> Router {
    let routes = Router::new()
        .route(
            "/api/admin/groups/{id}/model-policy",
            get(get_admin_group_model_policy).put(update_admin_group_model_policy),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .with_state(service)
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ));
    crate::router::build_router_with_routes_and_frontend(
        routes,
        Router::new(),
        config,
        256 * 1024,
        None,
    )
}

async fn get_admin_group_model_policy(
    State(service): State<Arc<dyn GroupModelPolicyService>>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let policy = service
        .get(
            authentication.principal(),
            crate::management_groups::parse_group_id(&group_id)?,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminGroupModelPolicyResponse::from(policy)))
}

async fn update_admin_group_model_policy(
    State(service): State<Arc<dyn GroupModelPolicyService>>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminGroupModelPolicyWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let target_group_id = crate::management_groups::parse_group_id(&group_id)?;
    let overrides = request
        .ratio_overrides
        .map(|items| {
            items
                .into_iter()
                .map(|item| {
                    let source_group_id = GroupId::new(item.source_group_id)
                        .map_err(|_| ManagementError::InvalidRequest)?;
                    Ok(GroupModelRatioOverridePolicyRecord {
                        source_group_id,
                        target_group_id,
                        model: item.model,
                        ratio_micros: item.ratio_micros,
                    })
                })
                .collect::<Result<Vec<_>, ManagementError>>()
        })
        .transpose()?;
    let policy = service
        .replace(
            authentication.principal(),
            target_group_id,
            request.allowed_models,
            overrides,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminGroupModelPolicyResponse::from(policy)))
}

fn map_error(error: GroupModelPolicyError) -> ManagementError {
    match error {
        GroupModelPolicyError::InvalidInput => ManagementError::InvalidRequest,
        GroupModelPolicyError::Forbidden => ManagementError::Forbidden,
        GroupModelPolicyError::NotFound => ManagementError::GroupNotFound,
        GroupModelPolicyError::Internal | GroupModelPolicyError::RuntimeRefreshFailed => {
            ManagementError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{
        GroupModelPolicyFuture, LoginCredentials, SessionAuthenticationError,
        SessionAuthenticationFuture, SessionLoginFuture, SessionPrincipal, SessionRole,
    };
    use af_domain::{GroupId, UserId};
    use axum::body::{Body, to_bytes};
    use http::{Request, StatusCode, header::AUTHORIZATION};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    struct Authenticator;

    impl SessionAuthenticator for Authenticator {
        fn login<'a>(&'a self, _: &'a LoginCredentials) -> SessionLoginFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
        }

        fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
            Box::pin(async move {
                let role = match token {
                    "user" => SessionRole::User,
                    "admin" => SessionRole::Admin,
                    _ => return Err(SessionAuthenticationError::InvalidSession),
                };
                Ok(SessionAuthentication::new(
                    SessionPrincipal::new(UserId::new(1).unwrap(), role),
                    GroupId::new(10).unwrap(),
                    4_000_000_000,
                ))
            })
        }
    }

    struct Service {
        calls: AtomicUsize,
    }

    impl GroupModelPolicyService for Service {
        fn get(
            &self,
            _: af_admin::SessionPrincipal,
            group_id: GroupId,
        ) -> GroupModelPolicyFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(GroupModelPolicyRecord {
                    group_id,
                    allowed_models: Some(vec!["gpt-test".to_owned()]),
                    ratio_overrides: vec![],
                })
            })
        }

        fn replace(
            &self,
            _: af_admin::SessionPrincipal,
            group_id: GroupId,
            allowed_models: Option<Vec<String>>,
            _: Option<Vec<af_admin::GroupModelRatioOverridePolicyRecord>>,
        ) -> GroupModelPolicyFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(GroupModelPolicyRecord {
                    group_id,
                    allowed_models,
                    ratio_overrides: vec![],
                })
            })
        }
    }

    #[tokio::test]
    async fn policy_routes_require_admin_and_return_no_store() {
        let service = Arc::new(Service {
            calls: AtomicUsize::new(0),
        });
        let config = af_config::ServerConfig::default();
        let app =
            build_group_model_policy_router(service.clone(), Arc::new(Authenticator), &config);
        for (method, token) in [("GET", "user"), ("PUT", "user")] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/api/admin/groups/10/model-policy")
                        .header(AUTHORIZATION, format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"allowed_models":["gpt-test"]}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(service.calls.load(Ordering::SeqCst), 0);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/admin/groups/10/model-policy")
                    .header(AUTHORIZATION, "Bearer admin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[http::header::CACHE_CONTROL], "no-store");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 32 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"group_id": 10, "allowed_models": ["gpt-test"], "ratio_overrides": []})
        );
        assert_eq!(service.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn request_rejects_missing_or_unknown_fields() {
        assert!(
            serde_json::from_value::<AdminGroupModelPolicyWriteRequest>(serde_json::json!({}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<AdminGroupModelPolicyWriteRequest>(
                serde_json::json!({"allowed_models": [], "flags": {}})
            )
            .is_err()
        );
    }
}
