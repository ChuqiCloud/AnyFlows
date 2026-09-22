import {
  clearManagementSessionToken,
  getManagementSessionToken,
  setManagementSessionToken,
} from "@/lib/api/session-token";
import type { PrincipalProfile } from "@/shared/session/contracts";

/**
 * 访问令牌统一存放在业务层 session-token，平台只做转发，
 * 避免平台与业务各持一份令牌、401 后状态互相不一致。
 */
class TokenManager {
  private readonly PRINCIPAL_KEY = "auth_principal";

  setAccessToken(token: string) {
    setManagementSessionToken(token);
  }

  getAccessToken(): string | null {
    return getManagementSessionToken() ?? null;
  }

  clearTokens() {
    clearManagementSessionToken();
    sessionStorage.removeItem(this.PRINCIPAL_KEY);
  }

  setPrincipal(principal: PrincipalProfile) {
    sessionStorage.setItem(this.PRINCIPAL_KEY, JSON.stringify(principal));
  }

  getPrincipal(): PrincipalProfile | null {
    const principalStr = sessionStorage.getItem(this.PRINCIPAL_KEY);

    if (!principalStr) {
      return null;
    }

    try {
      return JSON.parse(principalStr) as PrincipalProfile;
    } catch {
      return null;
    }
  }
}

export const tokenManager = new TokenManager();
