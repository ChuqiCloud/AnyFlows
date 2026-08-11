import type {
  AdminChannelType,
  AdminCredential,
  AdminCredentialCreateRequestWritable,
} from '@/lib/api/generated/types.gen'

export type WritableCredentialKind = AdminCredentialCreateRequestWritable['kind']

export type CredentialRuntimeState =
  | 'available'
  | 'authorizationPending'
  | 'autoDisabled'
  | 'cooling'
  | 'disabled'
  | 'paused'
  | 'pending'

export const writableCredentialKinds = [
  'api_key',
  'oauth',
  'bedrock',
  'service_account',
] as const satisfies readonly WritableCredentialKind[]

/** 将开放的持久化类型收窄为管理端当前可写的四类凭据。 */
export function asWritableCredentialKind(value: AdminCredential['kind']) {
  return writableCredentialKinds.find((kind) => kind === value)
}

/** 依据适配器认证契约限制新建类型，避免保存必然无法运行的组合。 */
export function credentialKindsForChannel(channelType: AdminChannelType): readonly WritableCredentialKind[] {
  switch (channelType) {
    case 'bedrock':
      return ['bedrock']
    case 'vertex':
      return ['oauth', 'service_account']
    case 'anthropic':
    case 'gemini':
    case 'openai':
      return ['api_key', 'oauth']
    case 'jina':
    case 'cohere':
    case 'xai':
      return ['api_key']
    case 'custom':
      return ['api_key', 'oauth']
  }
}

/** 从持久化状态与冷却窗口推导真实运行状态，不用前端健康分数猜测。 */
export function credentialRuntimeState(
  credential: AdminCredential,
  nowSeconds = Math.floor(Date.now() / 1_000),
): CredentialRuntimeState {
  if (credential.oauth_token_pending) return 'authorizationPending'
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

/** 返回当前凭据所有临时冷却边界中的最晚时间。 */
export function credentialCoolingUntil(credential: AdminCredential) {
  const value = Math.max(
    credential.rate_limit_reset_at ?? 0,
    credential.overload_until ?? 0,
    credential.temp_unschedulable_until ?? 0,
  )
  return value > 0 ? value : undefined
}

/** 只把同渠道、同类型且自身不是影子账号的凭据作为可选父账号。 */
export function credentialParentCandidates(
  credentials: readonly AdminCredential[],
  kind: WritableCredentialKind,
  currentId?: number,
) {
  return credentials.filter((credential) => credential.id !== currentId
    && credential.kind === kind
    && credential.parent_id === null)
}
