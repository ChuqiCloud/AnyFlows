export type TenantRole =
  | "owner"
  | "admin"
  | "member"
  | "service"
  | "auditor"
  | "job";

export interface TenantMembership {
  tenant_id: string;
  code: string;
  name: string;
  roles: TenantRole[];
  role?: TenantRole;
  status: "active" | "suspended" | "disabled" | "expired" | "deleted";
  expires_at?: string | null;
  is_default?: boolean;
}
