import type { TenantMembership } from "./types";

const getActiveTenants = (tenants: TenantMembership[]) =>
  tenants.filter((tenant) => tenant.status === "active");

export const resolveCurrentTenantId = (
  tenants: TenantMembership[],
  preferredTenantId?: string | null,
) => {
  const activeTenants = getActiveTenants(tenants);

  if (
    preferredTenantId &&
    activeTenants.some((tenant) => tenant.tenant_id === preferredTenantId)
  ) {
    return preferredTenantId;
  }

  return (
    activeTenants.find((tenant) => tenant.is_default)?.tenant_id ??
    activeTenants[0]?.tenant_id ??
    null
  );
};
