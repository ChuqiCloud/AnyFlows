import type {
  AdminChannel,
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

/** 仅原生 OpenAI Responses 渠道允许派生 Spark 影子。 */
export function supportsSparkShadow(channel: Pick<AdminChannel, 'type' | 'protocol'>) {
  return channel.type === 'openai' && channel.protocol === 'openai_responses'
}

/** 额度维度是管理端识别影子的稳定事实；损坏记录也不会重新暴露密钥入口。 */
export function isSparkShadowCredential(credential: AdminCredential) {
  return credential.quota_dimension === 'spark'
}

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
export function credentialKindsForChannel(
  channelType: AdminChannelType,
  provider?: string | null,
): readonly WritableCredentialKind[] {
  if (channelType === 'openai' && provider?.trim().toLowerCase() === 'codex') return ['oauth']
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

/** 只返回结构有效、已启用且尚未派生影子的 OAuth 根凭据。 */
export function sparkShadowParentCandidates(
  credentials: readonly AdminCredential[],
  currentShadowId?: number,
) {
  return credentials.filter((credential) => credential.kind === 'oauth'
    && credential.status === 'enabled'
    && !credential.oauth_token_pending
    && !credential.blocks_spark_shadow
    && credential.parent_id === null
    && credential.quota_dimension === 'global'
    && !credentials.some((child) => child.parent_id === credential.id
      && child.id !== currentShadowId))
}

/** 使用服务端派生事实判断母凭据是否阻止影子，避免把普通额度冷却误判为共享故障。 */
export function sparkShadowParentBlocked(parent: AdminCredential | undefined) {
  return parent === undefined || parent.blocks_spark_shadow
}
