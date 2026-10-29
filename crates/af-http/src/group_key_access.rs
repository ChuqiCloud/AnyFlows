use std::sync::Arc;

use af_admin::{
    GroupKeyAccessService, GroupKeyPolicy, GroupKeyPolicyCommand, GroupKeyPolicyError,
    SessionAuthentication, SessionAuthenticator, UserTokenGroupListQuery, UserTokenGroupPage,
};
use af_domain::UserId;
use axum::{
    Json, Router,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
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

/// 个人 Key 允许选择分组的范围。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GroupKeySelectionMode {
    DefaultOnly,
    AllUsers,
    SelectedUsers,
}

impl From<GroupKeySelectionMode> for af_admin::GroupKeySelectionMode {
    fn from(mode: GroupKeySelectionMode) -> Self {
        match mode {
            GroupKeySelectionMode::DefaultOnly => Self::DefaultOnly,
            GroupKeySelectionMode::AllUsers => Self::AllUsers,
            GroupKeySelectionMode::SelectedUsers => Self::SelectedUsers,
        }
    }
}

impl From<af_admin::GroupKeySelectionMode> for GroupKeySelectionMode {
    fn from(mode: af_admin::GroupKeySelectionMode) -> Self {
        match mode {
            af_admin::GroupKeySelectionMode::DefaultOnly => Self::DefaultOnly,
            af_admin::GroupKeySelectionMode::AllUsers => Self::AllUsers,
            af_admin::GroupKeySelectionMode::SelectedUsers => Self::SelectedUsers,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroupKeyPolicy)]
pub(crate) struct AdminGroupKeyPolicyResponse {
    #[schema(minimum = 1)]
    group_id: i64,
    selection_mode: GroupKeySelectionMode,
    #[schema(max_items = 1000)]
    allowed_user_ids: Vec<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminGroupKeyPolicyWriteRequest {
    selection_mode: GroupKeySelectionMode,
    #[serde(default)]
    #[schema(max_items = 1000)]
    allowed_user_ids: Vec<i64>,
}

impl AdminGroupKeyPolicyWriteRequest {
    fn into_command(self) -> Result<GroupKeyPolicyCommand, ManagementError> {
        if self.allowed_user_ids.len() > 1_000 {
            return Err(ManagementError::InvalidRequest);
        }
        let ids = self
            .allowed_user_ids
            .into_iter()
            .map(|id| UserId::new(id).map_err(|_| ManagementError::InvalidRequest))
            .collect::<Result<Vec<_>, _>>()?;
        GroupKeyPolicyCommand::new(self.selection_mode.into(), ids).map_err(map_error)
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTokenGroup)]
pub(crate) struct UserTokenGroupResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    /// 基础倍率，1000000 表示 1.0；分时倍率按实际请求时间另行计算。
    #[schema(minimum = 0)]
    ratio_micros: i64,
    is_default: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserTokenGroupListResponse {
    #[schema(max_items = 100)]
    groups: Vec<UserTokenGroupResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 构建自选分组接口，并应用会话、管理员角色及通用 Origin/请求预算边界。
pub fn build_group_key_access_router(
    service: Arc<dyn GroupKeyAccessService>,
    authenticator: Arc<dyn SessionAuthenticator>,
    config: &af_config::ServerConfig,
) -> Router {
    crate::router::build_router_with_routes_and_frontend(
        authenticated_routes(service, authenticator),
        Router::new(),
        config,
        32 * 1024,
        None,
    )
}

fn authenticated_routes(
    service: Arc<dyn GroupKeyAccessService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let admin = Router::new()
        .route(
            "/api/admin/groups/{id}/key-policy",
            get(get_admin_group_key_policy).put(update_admin_group_key_policy),
        )
        .route_layer(middleware::from_fn(authorize_management_admin));
    Router::new()
        .merge(admin)
        .route("/api/token-groups", get(list_user_token_groups))
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(service)
}

/// 管理员读取未删除分组的自选策略。
pub(crate) async fn get_admin_group_key_policy(
    State(service): State<Arc<dyn GroupKeyAccessService>>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let policy = service
        .get_policy(
            authentication.principal(),
            crate::management_groups::parse_group_id(&group_id)?,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminGroupKeyPolicyResponse::from(policy)))
}

/// 管理员原子替换个人 Key 自选策略与用户授权。
pub(crate) async fn update_admin_group_key_policy(
    State(service): State<Arc<dyn GroupKeyAccessService>>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminGroupKeyPolicyWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let policy = service
        .update_policy(
            authentication.principal(),
            crate::management_groups::parse_group_id(&group_id)?,
            request.into_command()?,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminGroupKeyPolicyResponse::from(policy)))
}

/// 返回当前登录用户可选分组的最小公开字段。
pub(crate) async fn list_user_token_groups(
    State(service): State<Arc<dyn GroupKeyAccessService>>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let page = service
        .list_user_groups(
            authentication.principal(),
            parse_list_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserTokenGroupListResponse::from(page)))
}

impl From<GroupKeyPolicy> for AdminGroupKeyPolicyResponse {
    fn from(policy: GroupKeyPolicy) -> Self {
        Self {
            group_id: policy.group_id.get(),
            selection_mode: policy.selection_mode.into(),
            allowed_user_ids: policy
                .allowed_user_ids
                .into_iter()
                .map(UserId::get)
                .collect(),
        }
    }
}

impl From<UserTokenGroupPage> for UserTokenGroupListResponse {
    fn from(page: UserTokenGroupPage) -> Self {
        Self {
            groups: page
                .groups
                .into_iter()
                .map(|group| UserTokenGroupResponse {
                    id: group.group_id.get(),
                    display_name: group.display_name,
                    ratio_micros: group.ratio_micros,
                    is_default: group.is_default,
                })
                .collect(),
            next_cursor: page.next_cursor.map(af_domain::GroupId::get),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<UserTokenGroupListQuery, ManagementError> {
    let Some(raw_query) = raw_query.filter(|query| !query.is_empty()) else {
        return Ok(UserTokenGroupListQuery::default());
    };
    let bytes = raw_query.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit())
        {
            return Err(ManagementError::InvalidRequest);
        }
    }
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(bytes) {
        match key.as_ref() {
            "after" if after.is_none() => {
                after = Some(crate::management_groups::parse_group_id(&value)?);
            }
            "limit"
                if limit.is_none()
                    && !value.is_empty()
                    && value.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| ManagementError::InvalidRequest)?,
                );
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    UserTokenGroupListQuery::new(after, limit.unwrap_or(50)).map_err(map_error)
}

fn map_error(error: GroupKeyPolicyError) -> ManagementError {
    match error {
        GroupKeyPolicyError::InvalidInput => ManagementError::InvalidRequest,
        GroupKeyPolicyError::Forbidden => ManagementError::Forbidden,
        GroupKeyPolicyError::NotFound => ManagementError::GroupNotFound,
        GroupKeyPolicyError::OwnerUnavailable => ManagementError::InvalidSession,
        GroupKeyPolicyError::Internal => ManagementError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{
        GroupKeyPolicyFuture, LoginCredentials, SessionAuthenticationError,
        SessionAuthenticationFuture, SessionLoginFuture, SessionPrincipal, SessionRole,
        UserTokenGroup, UserTokenGroupListFuture,
    };
    use af_domain::GroupId;
    use axum::body::{Body, to_bytes};
    use http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CACHE_CONTROL},
    };
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

    impl GroupKeyAccessService for Service {
        fn get_policy<'a>(
            &'a self,
            _: SessionPrincipal,
            group_id: GroupId,
        ) -> GroupKeyPolicyFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(GroupKeyPolicy {
                    group_id,
                    selection_mode: af_admin::GroupKeySelectionMode::DefaultOnly,
                    allowed_user_ids: vec![],
                })
            })
        }
        fn update_policy<'a>(
            &'a self,
            _: SessionPrincipal,
            _: GroupId,
            _: GroupKeyPolicyCommand,
        ) -> GroupKeyPolicyFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Err(GroupKeyPolicyError::Internal) })
        }
        fn list_user_groups<'a>(
            &'a self,
            _: SessionPrincipal,
            _: UserTokenGroupListQuery,
        ) -> UserTokenGroupListFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(UserTokenGroupPage {
                    groups: vec![UserTokenGroup {
                        group_id: GroupId::new(10).unwrap(),
                        display_name: "标准".to_owned(),
                        ratio_micros: 1_000_000,
                        is_default: true,
                    }],
                    next_cursor: None,
                })
            })
        }
    }

    #[tokio::test]
    async fn protected_routes_reject_guests_and_non_admins_before_service_calls() {
        let service = Arc::new(Service {
            calls: AtomicUsize::new(0),
        });
        let app = authenticated_routes(service.clone(), Arc::new(Authenticator));
        for path in ["/api/token-groups", "/api/admin/groups/10/key-policy"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for method in ["GET", "PUT"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri("/api/admin/groups/10/key-policy")
                        .header(AUTHORIZATION, "Bearer user")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(service.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn user_group_response_exposes_only_safe_fields_and_is_not_cached() {
        let service = Arc::new(Service {
            calls: AtomicUsize::new(0),
        });
        let app = authenticated_routes(service, Arc::new(Authenticator));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/token-groups")
                    .header(AUTHORIZATION, "Bearer user")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 16 * 1024).await.unwrap())
                .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"groups": [{"id": 10, "display_name": "标准", "ratio_micros": 1000000, "is_default": true}], "next_cursor": null})
        );
    }

    #[test]
    fn list_parser_rejects_ambiguous_and_malformed_queries() {
        for query in [
            "after=1&after=2",
            "limit=1&limit=2",
            "limit=0",
            "limit=101",
            "limit=-1",
            "after=0",
            "unexpected=1",
            "limit=%",
            "after=%FF",
        ] {
            assert!(parse_list_query(Some(query)).is_err(), "{query}");
        }
        assert!(parse_list_query(Some("after=1&limit=100")).is_ok());
    }

    #[test]
    fn policy_json_rejects_unknown_modes_fields_and_invalid_user_ids() {
        assert!(
            serde_json::from_value::<AdminGroupKeyPolicyWriteRequest>(
                serde_json::json!({"selection_mode": "public"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<AdminGroupKeyPolicyWriteRequest>(
                serde_json::json!({"selection_mode": "all_users", "flags": {}})
            )
            .is_err()
        );
        let request = serde_json::from_value::<AdminGroupKeyPolicyWriteRequest>(
            serde_json::json!({"selection_mode": "selected_users", "allowed_user_ids": [0]}),
        )
        .unwrap();
        assert!(matches!(
            request.into_command(),
            Err(ManagementError::InvalidRequest)
        ));
    }
}
