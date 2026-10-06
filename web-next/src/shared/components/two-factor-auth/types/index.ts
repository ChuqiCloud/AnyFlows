export type TwoFactorAuthMethod = "sms" | "email" | "totp" | "wechat";

export type VerificationStatus =
  | "idle"
  | "sending"
  | "verifying"
  | "success"
  | "error";

export type VerificationResultStatus = "pending" | "success" | "error";

export interface AuthMethodConfig {
  method: TwoFactorAuthMethod;
  label: string;
  icon: string;
  enabled: boolean;
}

export interface VerificationRequest {
  method: TwoFactorAuthMethod;
  code?: string;
  qrCodeUrl?: string;
}

export interface VerificationResponse {
  success: boolean;
  status?: VerificationResultStatus;
  message?: string;
  token?: string;
  qrCodeUrl?: string;
}

export type OnVerifyCallback = (
  request: VerificationRequest,
) => Promise<VerificationResponse> | VerificationResponse;

export type OnVerifySuccess = (response: VerificationResponse) => void;

export type OnVerifyError = (error: Error | string) => void;
