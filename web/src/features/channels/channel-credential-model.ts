import type {
  AdminCredential,
  AdminCredentialCreateRequestWritable,
  AdminCredentialUpdateRequestWritable,
  AdminRoutingWriteStatus,
} from '@/lib/api/generated/types.gen'

export type CredentialRuntimeState =
  | 'available'
  | 'autoDisabled'
  | 'cooling'
  | 'disabled'
  | 'paused'
  | 'pending'

/** 校验服务端允许的 API Key 明文边界，不在错误中保留原始输入。 */
export function isValidApiKey(value: string) {
  return value.length > 0
    && value.length <= 16 * 1024
    && value.trim() === value
    && /^[\x20-\x7E]+$/.test(value)
}

/** 使用当前单 Key 调度默认值创建一条独立、可调度的加密凭据。 */
export function createApiKeyRequest(secret: string): AdminCredentialCreateRequestWritable {
  return {
    kind: 'api_key',
    secret,
    status: 'enabled',
    multi_key_mode: null,
    priority: 0,
    weight: 10,
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

/** 完整保留凭据调度元数据，仅切换人工启停状态并保留原密文。 */
export function updateCredentialStatusRequest(
  credential: AdminCredential,
  status: AdminRoutingWriteStatus,
): AdminCredentialUpdateRequestWritable | undefined {
  if (credential.kind !== 'api_key' && credential.kind !== 'oauth') return undefined
  return {
    kind: credential.kind,
    secret: null,
    status,
    multi_key_mode: credential.multi_key_mode,
    priority: credential.priority,
    weight: credential.weight,
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
