import type {
  AdminCredential,
  AdminCredentialCreateRequestWritable,
  AdminCredentialUpdateRequestWritable,
  AdminRoutingWriteStatus,
} from '@/lib/api/generated/types.gen'
import { asWritableCredentialKind, type WritableCredentialKind } from './credential-model.ts'
import {
  formatMicros,
  optionalInteger,
  optionalMicros,
  optionalNumber,
  optionalText,
} from './credential-form-scalar.ts'
import type { CredentialFormValues } from './credential-form-types'
import { isSparkShadowCredential } from './credential-model.ts'

/** 从脱敏凭据恢复非敏感字段；密文永远不会回填。 */
export function credentialFormValues(
  credential?: AdminCredential,
  initialKind: WritableCredentialKind = 'api_key',
): CredentialFormValues {
  const kind = credential ? asWritableCredentialKind(credential.kind) ?? initialKind : initialKind
  return {
    mode: credential && isSparkShadowCredential(credential) ? 'spark_shadow' : 'standard',
    kind,
    oauthCreateMode: credential === undefined && initialKind === 'oauth'
      ? 'authorize'
      : 'access_token',
    rotateSecret: credential === undefined,
    apiKey: '', accessToken: '', accessKeyId: '', secretAccessKey: '', sessionToken: '',
    clientEmail: '', privateKeyId: '', privateKey: '',
    status: credential?.status ?? 'enabled',
    schedulable: credential?.schedulable ?? true,
    multiKeyMode: credential?.multi_key_mode ?? 'none',
    priority: String(credential?.priority ?? 0),
    weight: String(credential?.weight ?? 10),
    concurrency: optionalNumber(credential?.concurrency),
    loadFactor: formatMicros(credential?.load_factor_micros),
    rateMultiplier: formatMicros(credential?.rate_multiplier_micros),
    parentId: optionalNumber(credential?.parent_id),
    proxyId: optionalNumber(credential?.proxy_id),
    quotaDimension: credential?.quota_dimension ?? 'global',
    oauthProvider: credential?.oauth_provider ?? '',
    oauthAccountKey: credential?.oauth_account_key ?? '',
    oauthProjectId: credential?.oauth_project_id ?? '',
  }
}

export function createCredentialRequest(values: CredentialFormValues): AdminCredentialCreateRequestWritable {
  if (values.mode === 'spark_shadow') {
    return {
      ...writeSchedulingFields(values),
      kind: 'oauth',
      secret: null,
      concurrency: null,
      parent_id: optionalInteger(values.parentId),
      quota_dimension: 'spark',
      proxy_id: null,
      oauth_provider: null,
      oauth_account_key: null,
      oauth_project_id: null,
    }
  }
  return {
    ...writeSchedulingFields(values),
    kind: values.kind,
    secret: credentialSecret(values),
    concurrency: optionalInteger(values.concurrency),
    parent_id: null,
    quota_dimension: 'global',
    proxy_id: optionalInteger(values.proxyId),
    oauth_provider: values.kind === 'oauth' ? optionalText(values.oauthProvider) : null,
    oauth_account_key: values.kind === 'oauth' ? optionalText(values.oauthAccountKey) : null,
    oauth_project_id: values.kind === 'oauth' ? optionalText(values.oauthProjectId) : null,
  }
}

export function updateCredentialRequest(
  values: CredentialFormValues,
  credential: AdminCredential,
): AdminCredentialUpdateRequestWritable {
  const shadow = isSparkShadowCredential(credential)
  const kind = asWritableCredentialKind(credential.kind) ?? values.kind
  return {
    ...writeSchedulingFields(values),
    kind,
    secret: shadow || !values.rotateSecret ? null : credentialSecret(values),
    concurrency: shadow ? null : optionalInteger(values.concurrency),
    parent_id: shadow ? credential.parent_id : null,
    quota_dimension: shadow ? 'spark' : 'global',
    proxy_id: shadow ? null : optionalInteger(values.proxyId),
    oauth_provider: !shadow && kind === 'oauth' ? optionalText(values.oauthProvider) : null,
    oauth_account_key: !shadow && kind === 'oauth' ? optionalText(values.oauthAccountKey) : null,
    oauth_project_id: !shadow && kind === 'oauth' ? optionalText(values.oauthProjectId) : null,
  }
}

/** 快捷启停同样完整保留所有调度字段与原密文。 */
export function updateCredentialStatusRequest(
  credential: AdminCredential,
  status: AdminRoutingWriteStatus,
): AdminCredentialUpdateRequestWritable | undefined {
  if (credential.oauth_token_pending) return undefined
  const kind = asWritableCredentialKind(credential.kind)
  if (!kind) return undefined
  const shadow = isSparkShadowCredential(credential)
  return {
    kind, secret: null, status,
    multi_key_mode: credential.multi_key_mode,
    priority: credential.priority,
    weight: credential.weight,
    concurrency: shadow ? null : credential.concurrency,
    load_factor_micros: credential.load_factor_micros,
    rate_multiplier_micros: credential.rate_multiplier_micros,
    schedulable: credential.schedulable,
    parent_id: shadow ? credential.parent_id : null,
    quota_dimension: shadow ? 'spark' : 'global',
    proxy_id: shadow ? null : credential.proxy_id,
    oauth_provider: !shadow && kind === 'oauth' ? credential.oauth_provider : null,
    oauth_account_key: !shadow && kind === 'oauth' ? credential.oauth_account_key : null,
    oauth_project_id: !shadow && kind === 'oauth' ? credential.oauth_project_id : null,
  }
}

function writeSchedulingFields(values: CredentialFormValues) {
  if (values.status === 'auto_disabled') throw new Error('自动停用凭据必须显式选择写入状态')
  return {
    status: values.status,
    multi_key_mode: values.multiKeyMode === 'none' ? null : values.multiKeyMode,
    priority: Number(values.priority),
    weight: Number(values.weight),
    load_factor_micros: optionalMicros(values.loadFactor),
    rate_multiplier_micros: optionalMicros(values.rateMultiplier),
    schedulable: values.schedulable,
  }
}

function credentialSecret(values: CredentialFormValues): AdminCredentialCreateRequestWritable['secret'] {
  switch (values.kind) {
    case 'api_key': return { kind: 'api_key', api_key: values.apiKey }
    case 'oauth': return values.oauthCreateMode === 'authorize'
      ? null
      : { kind: 'oauth', access_token: values.accessToken }
    case 'bedrock': return { kind: 'bedrock', access_key_id: values.accessKeyId, secret_access_key: values.secretAccessKey, session_token: optionalText(values.sessionToken) }
    case 'service_account': return { kind: 'service_account', client_email: values.clientEmail, private_key_id: optionalText(values.privateKeyId), private_key: values.privateKey }
  }
}
