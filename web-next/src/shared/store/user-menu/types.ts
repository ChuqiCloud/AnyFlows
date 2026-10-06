/**
 * 用户菜单所需的用户基本信息
 */
export interface UserMenuInfo {
  principalId: string;
  displayName: string;
  principalType: string;
  phone: string;
  email: string | null;
  phoneVerified: boolean;
  emailVerified: boolean;
}

/**
 * 用户菜单所需的实名状态
 */
export interface UserMenuRealnameStatus {
  type: "individual" | "enterprise" | "";
  status: "pending" | "verifying" | "approved" | "rejected" | "none";
  realName?: string;
  companyName?: string;
}

/**
 * 后端返回的主体详情（精简版）
 */
export interface UserDetailResponse {
  principal_id: string;
  principal_type: string;
  display_name: string;
  phone: string;
  email: string | null;
  phone_verified: boolean;
  email_verified: boolean;
}

/**
 * 后端返回的实名状态（新API格式）
 */
export interface RealnameStatusResponse {
  has_verification: boolean;
  verification?: {
    id: string;
    user_id: string;
    type: "personal" | "enterprise";
    status: "pending" | "verifying" | "approved" | "rejected";
    real_name: string;
    company_name?: string;
  };
}
