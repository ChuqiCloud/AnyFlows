import type { SessionUser } from "@/lib/api/generated/types.gen";
import { getManagementSession } from "@/lib/api/generated/sdk.gen";
import { apiClient } from "@/lib/api";
import type {
  AccessControlBootstrap,
  PrincipalProfile,
  RealnameBootstrap,
  SessionContext,
} from "./contracts";
import type { TenantMembership, TenantRole } from "@/shared/tenant";

const tenantRoleValues: readonly TenantRole[] = [
  "owner",
  "admin",
  "member",
  "service",
  "auditor",
  "job",
];

const defaultAccessControl: AccessControlBootstrap = {
  capabilities: [],
  features: [],
  surface: "user",
};

const defaultRealname: RealnameBootstrap = {
  applicable: false,
  status_code: "unverified",
  action_code: "none",
  type_code: "none",
  has_record: false,
  is_verified: false,
};

let cachedSessionContext: SessionContext | null = null;
let inFlightSessionContext: Promise<SessionContext> | null = null;

const isTenantRole = (value: string): value is TenantRole =>
  tenantRoleValues.includes(value as TenantRole);

const tenantStatusPriority = (status?: string | null) => {
  switch ((status ?? "").trim()) {
    case "active":
      return 4;
    case "suspended":
      return 3;
    case "disabled":
      return 2;
    case "expired":
      return 1;
    default:
      return 0;
  }
};

const normalizeTenantRoles = (
  roles?: unknown,
  fallbackRole?: unknown,
): TenantRole[] => {
  const source = Array.isArray(roles)
    ? roles
    : roles !== undefined
      ? [roles]
      : fallbackRole !== undefined
        ? [fallbackRole]
        : [];

  const uniqueRoles = new Set<TenantRole>();

  for (const candidate of source) {
    if (typeof candidate !== "string") {
      continue;
    }

    const role = candidate.trim();
    if (isTenantRole(role)) {
      uniqueRoles.add(role);
    }
  }

  return Array.from(uniqueRoles);
};

const mergeTenantMemberships = (
  tenants: unknown,
  defaultTenantId?: string | null,
): TenantMembership[] => {
  const tenantList = Array.isArray(tenants) ? tenants : [];
  const merged = new Map<string, TenantMembership>();
  const normalizedDefaultTenantId =
    typeof defaultTenantId === "string" ? defaultTenantId.trim() : "";

  for (const entry of tenantList) {
    if (!entry || typeof entry !== "object") {
      continue;
    }

    const tenant = entry as TenantMembership & {
      role?: unknown;
      roles?: unknown;
    };
    const tenantId =
      typeof tenant.tenant_id === "string" ? tenant.tenant_id.trim() : "";

    if (!tenantId) {
      continue;
    }

    const nextRoles = normalizeTenantRoles(tenant.roles, tenant.role);
    const nextStatus =
      typeof tenant.status === "string" && tenant.status.trim()
        ? tenant.status
        : "active";
    const nextExpiresAt =
      typeof tenant.expires_at === "string" ? tenant.expires_at : null;
    const nextIsDefault =
      Boolean(tenant.is_default) ||
      (normalizedDefaultTenantId !== "" &&
        tenantId === normalizedDefaultTenantId);

    const current = merged.get(tenantId);
    if (!current) {
      merged.set(tenantId, {
        tenant_id: tenantId,
        code: typeof tenant.code === "string" ? tenant.code : "",
        name: typeof tenant.name === "string" ? tenant.name : "",
        roles: nextRoles,
        status: nextStatus,
        expires_at: nextExpiresAt,
        is_default: nextIsDefault,
      });
      continue;
    }

    current.roles = Array.from(new Set([...current.roles, ...nextRoles]));

    if (tenantStatusPriority(nextStatus) > tenantStatusPriority(current.status)) {
      current.status = nextStatus;
    }

    current.expires_at = current.expires_at ?? nextExpiresAt;
    current.is_default = Boolean(current.is_default || nextIsDefault);
  }

  return Array.from(merged.values());
};

const normalizeSessionContext = (
  sessionContext: SessionContext,
): SessionContext => {
  if (!sessionContext?.principal) {
    throw new Error("Invalid session context payload");
  }

  const tenants = mergeTenantMemberships(
    sessionContext.tenants,
    sessionContext.default_tenant_id,
  );
  const accessControl = sessionContext.access_control ?? defaultAccessControl;
  const realname = sessionContext.realname ?? defaultRealname;
  const defaultTenantId =
    typeof sessionContext.default_tenant_id === "string" &&
    tenants.some((tenant) => tenant.tenant_id === sessionContext.default_tenant_id)
      ? sessionContext.default_tenant_id
      : null;

  return {
    ...sessionContext,
    tenants,
    default_tenant_id: defaultTenantId,
    access_control: {
      capabilities: Array.isArray(accessControl.capabilities)
        ? accessControl.capabilities
        : [],
      features: Array.isArray(accessControl.features)
        ? accessControl.features
        : [],
      surface: accessControl.surface ?? "user",
    },
    realname: {
      applicable: Boolean(realname.applicable),
      status_code: realname.status_code ?? "unverified",
      action_code: realname.action_code ?? "none",
      type_code: realname.type_code ?? "none",
      reject_reason: realname.reject_reason,
      has_record: Boolean(realname.has_record),
      is_verified: Boolean(realname.is_verified),
    },
  };
};

/**
 * AnyFlows 会话只返回用户 id 与角色，其余字段沿用安全默认值，
 * 平台契约保持不变，业务仍从平台读取主体信息。
 */
const toPrincipalProfile = (user: SessionUser): PrincipalProfile => ({
  principal_id: String(user.id),
  principal_type: user.role === "admin" ? "admin" : "user",
  display_name: String(user.id),
  phone: "",
  email: null,
  status: "active",
  phone_verified: false,
  email_verified: false,
  two_factor_enabled: false,
  login_attempts: 0,
  locked_until: null,
  last_login_at: null,
  last_login_ip: "",
  created_at: "",
  updated_at: "",
});

/** 权限面直接由角色推导：管理员进管理页，普通用户只进用户工作台。 */
const toAccessControlBootstrap = (
  user: SessionUser,
): AccessControlBootstrap => ({
  capabilities: [],
  features: [],
  surface: user.role === "admin" ? "admin" : "user",
});

const fetchSessionContext = async (): Promise<SessionContext> => {
  const { data } = await getManagementSession({ client: apiClient });

  return normalizeSessionContext({
    // AnyFlows 没有租户体系，保留字段以兼容平台契约。
    tenants: [],
    default_tenant_id: null,
    principal: toPrincipalProfile(data.user),
    access_control: toAccessControlBootstrap(data.user),
  });
};

export const clearSessionContextCache = () => {
  cachedSessionContext = null;
  inFlightSessionContext = null;
};

export const getSessionContext = async (
  options?: {
    forceRefresh?: boolean;
  },
): Promise<SessionContext> => {
  if (!options?.forceRefresh) {
    if (cachedSessionContext) {
      return cachedSessionContext;
    }

    if (inFlightSessionContext) {
      return inFlightSessionContext;
    }
  } else if (inFlightSessionContext) {
    return inFlightSessionContext;
  }

  const requestPromise = fetchSessionContext()
    .then((sessionContext) => {
      cachedSessionContext = sessionContext;
      return sessionContext;
    })
    .finally(() => {
      if (inFlightSessionContext === requestPromise) {
        inFlightSessionContext = null;
      }
    });

  inFlightSessionContext = requestPromise;

  return requestPromise;
};
