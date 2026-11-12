//! Personal access tokens use an independent system-API bearer scheme.

#![allow(dead_code, reason = "Document endpoints are consumed by utoipa macros")]

use crate::{
    access_token_error::PersonalAccessTokenErrorBody,
    access_tokens::{
        IssuedPersonalAccessTokenResponse, PersonalAccessTokenListResponse,
        PersonalAccessTokenPolicyRequest, PersonalAccessTokenResponse,
        PersonalAccessTokenScopeCatalogResponse, PersonalAccessTokenWriteRequest,
    },
};
use utoipa::OpenApi;

#[utoipa::path(get,path="/api/account/access-tokens",operation_id="listPersonalAccessTokens",tag="个人访问令牌",
    summary="读取我的系统 API 令牌",params(("after"=Option<i64>,Query,minimum=1),("limit"=Option<i64>,Query,minimum=1,maximum=100)),
    responses((status=200,body=PersonalAccessTokenListResponse),(status=401,body=PersonalAccessTokenErrorBody)),security(("bearerAuth"=[])))]
fn list() {}

#[utoipa::path(post,path="/api/account/access-tokens",operation_id="createPersonalAccessToken",tag="个人访问令牌",
    summary="签发系统 API 令牌",description="只接受交互会话和绑定本次授权参数的一次性验证证明。完整明文仅返回一次，不用于模型调用。",
    request_body=PersonalAccessTokenWriteRequest,responses((status=201,body=IssuedPersonalAccessTokenResponse<'static>),(status=403,body=PersonalAccessTokenErrorBody)),security(("bearerAuth"=[])))]
fn create() {}

#[utoipa::path(get,path="/api/account/access-tokens/scopes",operation_id="getPersonalAccessTokenScopes",tag="个人访问令牌",
    summary="读取当前账号可授权的系统 API 权限",responses((status=200,body=PersonalAccessTokenScopeCatalogResponse)),security(("bearerAuth"=[])))]
fn scopes() {}

#[utoipa::path(post,path="/api/account/access-tokens/verify",operation_id="verifyPersonalAccessTokenMutation",tag="个人访问令牌",
    summary="验证本次系统令牌敏感操作",description="需五分钟内完成交互登录；返回绑定用途、目标、会话和参数的一次性证明。",
    request_body=crate::access_token_proof::VerifyPersonalAccessTokenRequest,
    responses((status=200,body=crate::access_token_proof::PersonalAccessTokenProofResponse),(status=403,body=PersonalAccessTokenErrorBody)),security(("bearerAuth"=[])))]
fn verify() {}

#[utoipa::path(patch,path="/api/account/access-tokens/{id}",operation_id="updatePersonalAccessToken",tag="个人访问令牌",
    summary="更新我的系统 API 令牌",params(("id"=i64,Path,minimum=1)),request_body=PersonalAccessTokenWriteRequest,
    description="扩权、放宽来源IP、延期及重新启用要求一次性验证证明。普通退出不撤销令牌。",
    responses((status=200,body=PersonalAccessTokenResponse),(status=403,body=PersonalAccessTokenErrorBody),(status=409,body=PersonalAccessTokenErrorBody)),security(("bearerAuth"=[])))]
fn update() {}

#[utoipa::path(delete,path="/api/account/access-tokens/{id}",operation_id="revokePersonalAccessToken",tag="个人访问令牌",
    summary="撤销我的系统 API 令牌",params(("id"=i64,Path,minimum=1)),responses((status=204)),security(("bearerAuth"=[])))]
fn revoke() {}

#[utoipa::path(post,path="/api/account/access-tokens/revoke-all",operation_id="revokeOwnPersonalAccessTokens",tag="个人访问令牌",
    summary="撤销我的全部系统 API 令牌",responses((status=200,body=serde_json::Value)),security(("bearerAuth"=[])))]
fn revoke_own() {}

#[utoipa::path(get,path="/api/access-token/self",operation_id="getPersonalAccessTokenSelf",tag="个人访问令牌",
    summary="读取当前系统 API 令牌的权限和期限",responses((status=200,body=serde_json::Value),(status=401,body=PersonalAccessTokenErrorBody)),security(("personalAccessTokenBearer"=[])))]
fn token_self() {}

#[utoipa::path(get,path="/api/admin/access-tokens/policy",operation_id="getPersonalAccessTokenPolicy",tag="个人访问令牌",
    summary="读取系统 API 令牌全局策略",responses((status=200,body=PersonalAccessTokenPolicyRequest)),security(("bearerAuth"=[])))]
fn get_policy() {}

#[utoipa::path(put,path="/api/admin/access-tokens/policy",operation_id="updatePersonalAccessTokenPolicy",tag="个人访问令牌",
    summary="更新系统 API 令牌全局策略",description="只接受管理员交互会话。可以独立关闭签发或全部 PAT 调用，不影响模型 Key 与浏览器会话。",
    request_body=PersonalAccessTokenPolicyRequest,responses((status=200,body=PersonalAccessTokenPolicyRequest),(status=409,body=PersonalAccessTokenErrorBody)),security(("bearerAuth"=[])))]
fn update_policy() {}

#[utoipa::path(post,path="/api/admin/users/{user_id}/access-tokens/revoke-all",operation_id="revokeUserPersonalAccessTokens",tag="个人访问令牌",
    summary="应急撤销指定用户的全部系统 API 令牌",description="只接受管理员交互会话。撤销 PAT 不会撤销模型 Key。",
    params(("user_id"=i64,Path,minimum=1)),responses((status=200,body=serde_json::Value)),security(("bearerAuth"=[])))]
fn revoke_user() {}

#[derive(OpenApi)]
#[openapi(paths(list,create,scopes,verify,update,revoke,revoke_own,token_self,get_policy,update_policy,revoke_user),
    components(schemas(PersonalAccessTokenWriteRequest,PersonalAccessTokenResponse,PersonalAccessTokenListResponse,
        IssuedPersonalAccessTokenResponse<'static>,PersonalAccessTokenScopeCatalogResponse,PersonalAccessTokenPolicyRequest,
        crate::access_token_routes::AccessTokenScopeResponse,crate::access_token_routes::AccessTokenPresetResponse,
        crate::access_token_proof::VerifyPersonalAccessTokenRequest,crate::access_token_proof::PersonalAccessTokenProofResponse,
        PersonalAccessTokenErrorBody)))]
struct PersonalAccessTokenApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PersonalAccessTokenApi::openapi()
}
