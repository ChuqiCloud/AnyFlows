class TenantStorage {
  private readonly CURRENT_TENANT_ID_KEY = "current_tenant_id";

  setCurrentTenantId(tenantId: string | null) {
    if (tenantId) {
      sessionStorage.setItem(this.CURRENT_TENANT_ID_KEY, tenantId);

      return;
    }

    sessionStorage.removeItem(this.CURRENT_TENANT_ID_KEY);
  }

  getCurrentTenantId(): string | null {
    return sessionStorage.getItem(this.CURRENT_TENANT_ID_KEY);
  }

  clear() {
    sessionStorage.removeItem(this.CURRENT_TENANT_ID_KEY);
  }
}

export const tenantStorage = new TenantStorage();
