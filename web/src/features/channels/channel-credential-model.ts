import type {
  AdminCredential,
  AdminCredentialCreateRequestWritable,
  AdminCredentialUpdateRequestWritable,
  AdminOAuthProvider,
  AdminRoutingWriteStatus,
} from '@/lib/api/generated/types.gen'

export type CredentialRuntimeState =
  | 'available'
  | 'autoDisabled'
  | 'cooling'
  | 'disabled'
  | 'paused'
  | 'pending'

export type CredentialSchedule = {
  priority: number
  weight: number
}

/** 校验服务端允许的 API Key 或 OAuth access token 明文边界。 */
export function isValidCredentialSecret(value: string) {
  return value.length > 0
    && value.length <= 16 * 1024
    && value.trim() === value
    && /^[\x20-\x7E]+$/.test(value)
}

/** 校验凭据优先级与非负权重，保持和服务端整数约束一致。 */
export function isValidCredentialSchedule(schedule: CredentialSchedule) {
  return Number.isSafeInteger(schedule.priority)
    && Number.isSafeInteger(schedule.weight)
    && schedule.weight >= 0
}

/** 使用显式调度参数创建一条独立、可调度的加密凭据。 */
export function createApiKeyRequest(
  secret: string,
  schedule: CredentialSchedule = { priority: 0, weight: 10 },
): AdminCredentialCreateRequestWritable {
  return {
    kind: 'api_key',
    secret,
    status: 'enabled',
    multi_key_mode: null,
    priority: schedule.priority,
    weight: schedule.weight,
    concurrency: null,
    load_factor_micros: null,
    rate_multiplier_micros: null,
    schedulable: true,
    parent_id: null,
    quota_dimension: 'global',
    proxy_id: null,
    oauth_provider: null,
    oauth_account_key: null,
    oauth_project_id: null,
  }
}

/** 使用已有 access token 创建可继续执行官方授权的 OAuth 凭据。 */
export function createOAuthCredentialRequest(
  secret: string,
  provider: AdminOAuthProvider,
  schedule: CredentialSchedule = { priority: 0, weight: 10 },
): AdminCredentialCreateRequestWritable {
  return {
    kind: 'oauth',
    secret,
    status: 'enabled',
    multi_key_mode: null,
    priority: schedule.priority,
    weight: schedule.weight,
    concurrency: null,
    load_factor_micros: null,
    rate_multiplier_micros: null,
    schedulable: true,
    parent_id: null,
    quota_dimension: 'global',
    proxy_id: null,
    oauth_provider: provider,
    oauth_account_key: null,
    oauth_project_id: null,
  }
}

/** 完整保留凭据调度元数据，仅切换人工启停状态并保留原密文。 */
export function updateCredentialStatusRequest(
  credential: AdminCredential,
  status: AdminRoutingWriteStatus,
): AdminCredentialUpdateRequestWritable | undefined {
  return credentialUpdateRequest(credential, { status })
}

/** 完整保留敏感字段与扩展元数据，仅更新凭据调度优先级和权重。 */
export function updateCredentialScheduleRequest(
  credential: AdminCredential,
  schedule: CredentialSchedule,
): AdminCredentialUpdateRequestWritable | undefined {
  if (!isValidCredentialSchedule(schedule)) return undefined
  return credentialUpdateRequest(credential, schedule)
}

function credentialUpdateRequest(
  credential: AdminCredential,
  overrides: Partial<Pick<AdminCredentialUpdateRequestWritable, 'status' | 'priority' | 'weight'>>,
): AdminCredentialUpdateRequestWritable | undefined {
  if (credential.kind !== 'api_key' && credential.kind !== 'oauth') return undefined
  const status = overrides.status ?? credential.status
  if (status === 'auto_disabled') return undefined
  return {
    kind: credential.kind,
    secret: null,
    status,
    multi_key_mode: credential.multi_key_mode,
    priority: overrides.priority ?? credential.priority,
    weight: overrides.weight ?? credential.weight,
    concurrency: credential.concurrency,
    load_factor_micros: credential.load_factor_micros,
    rate_multiplier_micros: credential.rate_multiplier_micros,
    schedulable: credential.schedulable,
    parent_id: credential.parent_id,
    quota_dimension: credential.quota_dimension,
    proxy_id: credential.proxy_id,
    oauth_provider: credential.oauth_provider,
    oauth_account_key: credential.oauth_account_key,
    oauth_project_id: credential.oauth_project_id,
  }
}

/** 从已持久化的状态和冷却窗口推导当前真实可展示的运行状态。 */
export function credentialRuntimeState(
  credential: AdminCredential,
  nowSeconds = Math.floor(Date.now() / 1000),
): CredentialRuntimeState {
  if (credential.status === 'auto_disabled') return 'autoDisabled'
  if (credential.status !== 'enabled') return 'disabled'
  if (!credential.schedulable) return 'paused'
  const coolingUntil = Math.max(
    credential.rate_limit_reset_at ?? 0,
    credential.overload_until ?? 0,
    credential.temp_unschedulable_until ?? 0,
  )
  if (coolingUntil > nowSeconds) return 'cooling'
  return credential.last_used_at === null ? 'pending' : 'available'
}
