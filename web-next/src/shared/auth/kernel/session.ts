import { tokenManager } from "./token-store";

/**
 * 清理本地会话。令牌本身就是登录状态的唯一来源，
 * 因此清掉令牌即可，不需要额外的登出标记。
 */
export function clearStoredAuthSession() {
  tokenManager.clearTokens();
}

export type RestoreAuthSessionResult =
  | { ok: true; accessToken: string | null; refreshed: boolean }
  | { ok: false; reason: "no_session" };

/**
 * 恢复已登录状态。AnyFlows 没有刷新令牌，因此只判断本地是否还持有会话令牌；
 * 令牌是否仍然有效由业务侧的服务端会话查询确认。
 */
export function restoreAuthSession(): RestoreAuthSessionResult {
  const accessToken = tokenManager.getAccessToken();

  if (!accessToken) {
    return { ok: false, reason: "no_session" };
  }

  return { ok: true, accessToken, refreshed: false };
}
