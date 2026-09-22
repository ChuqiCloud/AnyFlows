export type PrincipalType = "user" | "service" | "admin" | "job";

export interface PrincipalAccountState {
  is_frozen: boolean;
  is_blacklisted: boolean;
  has_active_restrictions: boolean;
  restriction_types: string[];
  can_login: boolean;
}

export interface PrincipalProfile {
  principal_id: string;
  principal_type: PrincipalType;
  display_name: string;
  phone: string;
  email: string | null;
  status: string;
  phone_verified: boolean;
  email_verified: boolean;
  social_provider?: string | null;
  social_nickname?: string | null;
  social_avatar?: string | null;
  two_factor_enabled: boolean;
  login_attempts: number;
  locked_until: string | null;
  last_login_at: string | null;
  last_login_ip: string;
  account_state?: PrincipalAccountState;
  created_at: string;
  updated_at: string;
}

export interface RealnameBootstrap {
  applicable: boolean;
  status_code: "verified" | "unverified" | "recheck" | "abnormal";
  action_code: string;
  type_code: string;
  reject_reason?: string;
  has_record: boolean;
  is_verified: boolean;
}

export interface AccessControlBootstrap {
  capabilities: string[];
  features: string[];
  surface: import("@/shared/access-control").AccessSurface;
}

export interface AuthSession {
  accessToken: string | null;
}

export interface AuthContext {
  session: AuthSession;
  principal: PrincipalProfile | null;
}

export interface SessionContext {
  principal: PrincipalProfile;
  tenants: import("@/shared/tenant").TenantMembership[];
  default_tenant_id?: string | null;
  access_control?: AccessControlBootstrap;
  realname?: RealnameBootstrap;
}

export interface BaseResponse<T = unknown> {
  code: number;
  success: boolean;
  message: string;
  data: T;
  timestamp?: number;
  request_id?: string;
}

export interface ErrorResponse {
  code: number;
  success: boolean;
  error: {
    code: string;
    message: string;
    details?: Record<string, string>;
  };
  timestamp?: number;
  request_id?: string;
}

export interface LoginResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
  expires_at: string;
}

export interface RefreshTokenResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
  expires_at: string;
}
