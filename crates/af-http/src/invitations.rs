use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserInvitationError, UserInvitationService,
    UserInvitationSummary,
};
use axum::{
    Router,
    extract::{Extension, State},
    middleware,
    response::Response,
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct UserInvitationHttpState {
    service: Arc<dyn UserInvitationService>,
}

/// 当前用户邀请中心响应；最近记录不包含被邀请用户身份。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserInvitationSummaryResponse)]
pub(crate) struct UserInvitationSummaryResponse {
    #[schema(min_length = 25, max_length = 25, pattern = "^af-[A-Za-z0-9_-]{22}$")]
    invite_code: String,
    invited_count: u64,
    credited_count: u64,
    #[schema(minimum = 0)]
    current_rebate_quota: i64,
    #[schema(minimum = 0)]
    historical_rebate_quota: i64,
    recent_rebates: Vec<UserInvitationRebateResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserInvitationRebateResponse)]
pub(crate) struct UserInvitationRebateResponse {
    #[schema(minimum = 1)]
    quota_amount: i64,
    credited_at: u64,
}

/// 构建只允许有效登录会话访问的当前用户邀请路由。
pub(crate) fn build_user_invitation_router(
    service: Arc<dyn UserInvitationService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/account/invitations", get(get_invitations))
        .layer(authentication)
        .with_state(UserInvitationHttpState { service })
}

/// 返回当前会话用户自己的邀请码、汇总和脱敏返利记录。
async fn get_invitations(
    State(state): State<UserInvitationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let summary = state
        .service
        .get(authentication.principal())
        .await
        .map_err(map_invitation_error)?;
    Ok(no_store_json(UserInvitationSummaryResponse::from_summary(
        &summary,
    )))
}

impl UserInvitationSummaryResponse {
    fn from_summary(summary: &UserInvitationSummary) -> Self {
        Self {
            invite_code: summary.invite_code().to_owned(),
            invited_count: summary.invited_count(),
            credited_count: summary.credited_count(),
            current_rebate_quota: summary.current_rebate_quota().units(),
            historical_rebate_quota: summary.historical_rebate_quota().units(),
            recent_rebates: summary
                .recent_rebates()
                .iter()
                .map(|rebate| UserInvitationRebateResponse {
                    quota_amount: rebate.quota_amount().units(),
                    credited_at: rebate.credited_at(),
                })
                .collect(),
        }
    }
}

fn map_invitation_error(error: UserInvitationError) -> ManagementError {
    match error {
        UserInvitationError::InvalidSession => ManagementError::InvalidSession,
        UserInvitationError::Internal => ManagementError::Internal,
    }
}
