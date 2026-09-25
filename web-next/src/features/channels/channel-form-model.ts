import { z } from 'zod'

import type {
  AdminChannel,
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

export type ChannelEditorMode = 'create' | 'update'
export type SensitiveEditMode = 'preserve' | 'replace' | 'clear'
export type ChannelType = 'openai' | 'codex_oauth' | 'anthropic' | 'gemini' | 'jina' | 'cohere' | 'xai'
export type ChannelProtocol = 'openai_chat' | 'openai_responses' | 'openai_embeddings' | 'openai_images' | 'openai_audio' | 'openai_speech' | 'anthropic' | 'gemini' | 'jina_rerank' | 'cohere_rerank' | 'xai_video'
export type ResponsesCompactMode = 'auto' | 'force_on' | 'force_off'
export type ClientSimulationProfile = '' | 'anthropic_cli_headers_v1'
export type ClientSimulationBodyProfile = '' | 'anthropic_cli_system_date_v1'
export const MAX_CHANNEL_TIMEOUT_SECONDS = 900

export const channelParameterNames = [
  'temperature',
  'top_p',
  'max_output_tokens',
  'stop_sequences',
] as const

export type ChannelParameterName = (typeof channelParameterNames)[number]

export type KeyValueRow = {
  id: string
  key: string
  value: string
}

export type ParameterRow = {
  id: string
  key: ChannelParameterName | ''
  value: string
  values: string[]
}

export type ChannelFormValues = {
  provider: string
  name: string
  baseUrl: string
  timeoutSeconds: string
  channelType: ChannelType
  protocol: ChannelProtocol
  tag: string
  status: 'enabled' | 'disabled'
  weight: number
  priority: number
  autoBan: boolean
  autoBanStatusCodes: number[]
  autoBanKeywords: string[]
  poolMode: boolean
  clientSimulationProfile: ClientSimulationProfile
  clientSimulationRiskAccepted: boolean
  clientSimulationBodyProfile: ClientSimulationBodyProfile
  clientSimulationBodyRiskAccepted: boolean
  responsesWebsocketEnabled: boolean
  responsesCompactMode: ResponsesCompactMode
  responsesCompactModelMapping: KeyValueRow[]
  models: string[]
  groupIds: number[]
  modelMapping: KeyValueRow[]
  paramOverride: ParameterRow[]
  headerOverrideMode: SensitiveEditMode
  headerOverride: KeyValueRow[]
}

type ValidationMessages = {
  duplicateKey: string
  invalidEntries: string
  invalidField: string
  invalidHeader: string
  invalidParameter: string
  invalidAutoBanRules: string
  invalidClientSimulationRisk: string
  invalidClientSimulationBodyRisk: string
  invalidRouting: string
  invalidTimeout: string
  invalidUrl: string
}

const keyValueRowSchema = z.object({
  id: z.string(),
  key: z.string(),
  value: z.string(),
})

const parameterRowSchema = z.object({
  id: z.string(),
  key: z.union([z.literal(''), z.enum(channelParameterNames)]),
  value: z.string(),
  values: z.array(z.string()),
})

/** 集中校验结构化编辑器与后端容量边界。 */
export function buildChannelFormSchema(
  mode: ChannelEditorMode,
  messages: ValidationMessages,
  existingClientSimulationProfile: ClientSimulationProfile = '',
  existingClientSimulationBodyProfile: ClientSimulationBodyProfile = '',
) {
  return z.object({
    provider: z.string().refine((value) => isBoundedText(value, 64)
      && ![...value].some((character) => /\p{Cc}/u.test(character)), messages.invalidField),
    name: z.string().trim().min(1, messages.invalidField).max(128, messages.invalidField),
    baseUrl: z.string().trim().max(2048, messages.invalidField),
    timeoutSeconds: z.string(),
    channelType: z.enum(['openai', 'codex_oauth', 'anthropic', 'gemini', 'jina', 'cohere', 'xai']),
    protocol: z.enum(['openai_chat', 'openai_responses', 'openai_embeddings', 'openai_images', 'openai_audio', 'openai_speech', 'anthropic', 'gemini', 'jina_rerank', 'cohere_rerank', 'xai_video']),
    tag: z.string().trim().max(64, messages.invalidField),
    status: z.enum(['enabled', 'disabled']),
    weight: z.number().int(messages.invalidField).min(0, messages.invalidField),
    priority: z.number().int(messages.invalidField),
    autoBan: z.boolean(),
    autoBanStatusCodes: z.array(z.number()),
    autoBanKeywords: z.array(z.string()),
    poolMode: z.boolean(),
    clientSimulationProfile: z.enum(['', 'anthropic_cli_headers_v1']),
    clientSimulationRiskAccepted: z.boolean(),
    clientSimulationBodyProfile: z.enum(['', 'anthropic_cli_system_date_v1']),
    clientSimulationBodyRiskAccepted: z.boolean(),
    responsesWebsocketEnabled: z.boolean(),
    responsesCompactMode: z.enum(['auto', 'force_on', 'force_off']),
    responsesCompactModelMapping: z.array(keyValueRowSchema),
    models: z.array(z.string()),
    groupIds: z.array(z.number()),
    modelMapping: z.array(keyValueRowSchema),
    paramOverride: z.array(parameterRowSchema),
    headerOverrideMode: z.enum(['preserve', 'replace', 'clear']),
    headerOverride: z.array(keyValueRowSchema),
  }).superRefine((values, context) => {
    const protocolMatchesType = values.channelType === 'codex_oauth'
      ? values.protocol === 'openai_responses'
      : values.channelType === 'anthropic'
      ? values.protocol === 'anthropic'
      : values.channelType === 'gemini'
        ? values.protocol === 'gemini'
        : values.channelType === 'jina'
          ? values.protocol === 'jina_rerank'
          : values.channelType === 'cohere'
            ? values.protocol === 'cohere_rerank'
            : values.channelType === 'xai'
              ? values.protocol === 'xai_video'
          : values.protocol === 'openai_chat'
          || values.protocol === 'openai_responses'
          || values.protocol === 'openai_embeddings'
          || values.protocol === 'openai_images'
          || values.protocol === 'openai_audio'
          || values.protocol === 'openai_speech'
    if (!protocolMatchesType) addIssue(context, 'protocol', messages.invalidField)
    const nativeAnthropic = values.channelType === 'anthropic' && values.protocol === 'anthropic'
    if (values.clientSimulationProfile && !nativeAnthropic) {
      addIssue(context, 'clientSimulationProfile', messages.invalidField)
    }
    if (values.clientSimulationBodyProfile
      && (!nativeAnthropic || values.clientSimulationProfile !== 'anthropic_cli_headers_v1')) {
      addIssue(context, 'clientSimulationBodyProfile', messages.invalidField)
    }
    const clientSimulationChanged = values.clientSimulationProfile !== existingClientSimulationProfile
    if (values.clientSimulationProfile && clientSimulationChanged
      && !values.clientSimulationRiskAccepted) {
      addIssue(
        context,
        'clientSimulationRiskAccepted',
        messages.invalidClientSimulationRisk,
      )
    }
    const clientSimulationBodyChanged = values.clientSimulationBodyProfile
      !== existingClientSimulationBodyProfile
    if (values.clientSimulationBodyProfile && clientSimulationBodyChanged
      && !values.clientSimulationBodyRiskAccepted) {
      addIssue(
        context,
        'clientSimulationBodyRiskAccepted',
        messages.invalidClientSimulationBodyRisk,
      )
    }
    if (values.responsesWebsocketEnabled
      && (values.channelType !== 'openai' || values.protocol !== 'openai_responses')) {
      addIssue(context, 'responsesWebsocketEnabled', messages.invalidField)
    }
    if (values.channelType === 'codex_oauth' && values.responsesWebsocketEnabled) {
      addIssue(context, 'responsesWebsocketEnabled', messages.invalidField)
    }
    const nativeResponses = (values.channelType === 'openai' || values.channelType === 'codex_oauth')
      && values.protocol === 'openai_responses'
    if (!nativeResponses && (
      values.responsesCompactMode !== 'auto'
      || values.responsesCompactModelMapping.length > 0
    )) {
      addIssue(context, 'responsesCompactMode', messages.invalidField)
    }
    if (!validAutoBanRules(values.autoBanStatusCodes, values.autoBanKeywords)) {
      addIssue(context, 'autoBanKeywords', messages.invalidAutoBanRules)
    }

    if (values.channelType !== 'codex_oauth' && values.baseUrl && !isSafeBaseUrl(values.baseUrl)) {
      addIssue(context, 'baseUrl', messages.invalidUrl)
    }
    if (!isValidOptionalTimeout(values.timeoutSeconds)) {
      addIssue(context, 'timeoutSeconds', messages.invalidTimeout)
    }

    const modelsValid = values.models.length <= 512
      && uniqueStrings(values.models)
      && values.models.every((model) => isBoundedText(model, 255))
    const groupsValid = values.groupIds.length <= 64
      && new Set(values.groupIds).size === values.groupIds.length
      && values.groupIds.every((id) => Number.isSafeInteger(id) && id > 0)
    const routingValid = modelsValid
      && groupsValid
      && (values.models.length === 0) === (values.groupIds.length === 0)
      && values.models.length * values.groupIds.length <= 4096
    if (!routingValid) addIssue(context, 'groupIds', messages.invalidRouting)

    validateStringRows(values.modelMapping, 'modelMapping', messages, context)
    validateStringRows(
      values.responsesCompactModelMapping,
      'responsesCompactModelMapping',
      messages,
      context,
    )
    validateParameterRows(values.paramOverride, values.protocol, messages, context)
    if (mode === 'create' || values.headerOverrideMode === 'replace') {
      validateHeaderRows(values.headerOverride, messages, context)
    }
  })
}

export function defaultChannelValues(channel?: AdminChannel): ChannelFormValues {
  const codexOAuth = channel?.type === 'openai'
    && channel.protocol === 'openai_responses'
    && channel.provider?.trim().toLowerCase() === 'codex'
  return {
    provider: codexOAuth ? 'codex' : channel?.provider ?? channel?.type ?? 'openai',
    name: channel?.name ?? '',
    baseUrl: codexOAuth ? '' : channel?.base_url ?? '',
    timeoutSeconds: channel?.timeout_secs == null ? '' : String(channel.timeout_secs),
    channelType: codexOAuth
      ? 'codex_oauth'
      : channel?.type === 'anthropic'
      ? 'anthropic'
      : channel?.type === 'gemini'
        ? 'gemini'
        : channel?.type === 'jina'
          ? 'jina'
          : channel?.type === 'cohere'
            ? 'cohere'
            : channel?.type === 'xai' ? 'xai' : 'openai',
    protocol: channel?.protocol === 'anthropic'
      ? 'anthropic'
      : channel?.protocol === 'gemini'
        ? 'gemini'
        : channel?.protocol === 'jina_rerank'
          ? 'jina_rerank'
          : channel?.protocol === 'cohere_rerank'
            ? 'cohere_rerank'
            : channel?.protocol === 'xai_video'
              ? 'xai_video'
          : channel?.protocol === 'openai_responses'
          ? 'openai_responses'
          : channel?.protocol === 'openai_embeddings'
            ? 'openai_embeddings'
            : channel?.protocol === 'openai_images'
              ? 'openai_images'
              : channel?.protocol === 'openai_audio'
                ? 'openai_audio'
                : channel?.protocol === 'openai_speech' ? 'openai_speech' : 'openai_chat',
    tag: channel?.tag ?? '',
    status: channel?.status === 'enabled' ? 'enabled' : 'disabled',
    weight: channel?.weight ?? 10,
    priority: channel?.priority ?? 0,
    autoBan: channel?.auto_ban ?? true,
    autoBanStatusCodes: [...(channel?.auto_ban_rules?.status_codes ?? [])],
    autoBanKeywords: [...(channel?.auto_ban_rules?.keywords ?? [])],
    poolMode: channel?.pool_mode ?? false,
    clientSimulationProfile: channel?.client_simulation_profile ?? '',
    clientSimulationRiskAccepted: false,
    clientSimulationBodyProfile: channel?.client_simulation_body_profile ?? '',
    clientSimulationBodyRiskAccepted: false,
    responsesWebsocketEnabled: channel?.responses_websocket_enabled ?? false,
    responsesCompactMode: channel?.responses_compact_mode ?? 'auto',
    responsesCompactModelMapping: stringMapRows(
      channel?.responses_compact_model_mapping ?? {},
      'compact-mapping',
    ),
    models: [...(channel?.models ?? [])],
    groupIds: [...(channel?.group_ids ?? [])],
    modelMapping: stringMapRows(channel?.model_mapping ?? {}, 'mapping'),
    paramOverride: parameterRows(channel?.param_override ?? {}),
    headerOverrideMode: channel ? 'preserve' : 'replace',
    headerOverride: [],
  }
}

export function toCreateRequest(values: ChannelFormValues): AdminChannelCreateRequestWritable {
  return {
    ...commonRequest(values),
    header_override: stringRowsObject(values.headerOverride),
    settings: {},
  }
}

export function toUpdateRequest(values: ChannelFormValues): AdminChannelUpdateRequestWritable {
  const headerOverride = sensitiveValue(
    values.headerOverrideMode,
    stringRowsObject(values.headerOverride),
  )
  return {
    ...commonRequest(values),
    ...(headerOverride !== undefined && { header_override: headerOverride }),
  }
}

export function createKeyValueRow(prefix: string): KeyValueRow {
  return { id: createRowId(prefix), key: '', value: '' }
}

export function createParameterRow(key: ChannelParameterName | '' = ''): ParameterRow {
  return {
    id: createRowId('parameter'),
    key,
    value: '',
    values: key === 'stop_sequences' ? [''] : [],
  }
}

function commonRequest(values: ChannelFormValues) {
  return {
    provider: values.channelType === 'codex_oauth' ? 'codex' : values.provider.trim(),
    name: values.name.trim(),
    type: values.channelType === 'codex_oauth' ? 'openai' : values.channelType,
    protocol: values.channelType === 'codex_oauth' ? 'openai_responses' : values.protocol,
    base_url: values.channelType === 'codex_oauth' ? null : values.baseUrl.trim() || null,
    timeout_secs: values.timeoutSeconds.trim() === '' ? null : Number(values.timeoutSeconds),
    status: values.status,
    weight: values.weight,
    priority: values.priority,
    auto_ban: values.autoBan,
    auto_ban_rules: {
      status_codes: values.autoBanStatusCodes,
      keywords: values.autoBanKeywords,
    },
    pool_mode: values.poolMode,
    client_simulation_profile: values.clientSimulationProfile || null,
    client_simulation_risk_accepted: values.clientSimulationRiskAccepted,
    client_simulation_body_profile: values.clientSimulationBodyProfile || null,
    client_simulation_body_risk_accepted: values.clientSimulationBodyRiskAccepted,
    responses_websocket_enabled: values.responsesWebsocketEnabled,
    responses_compact_mode: values.responsesCompactMode,
    responses_compact_model_mapping: stringRowsObject(values.responsesCompactModelMapping),
    models: values.models,
    group_ids: values.groupIds,
    model_mapping: stringRowsObject(values.modelMapping),
    param_override: parameterRowsObject(values.paramOverride),
    tag: values.tag.trim() || null,
  }
}

function sensitiveValue<T extends Record<string, unknown>>(
  mode: SensitiveEditMode,
  replacement: T,
): T | undefined {
  if (mode === 'preserve') return undefined
  return mode === 'clear' ? {} as T : replacement
}

function stringRowsObject(rows: KeyValueRow[]): Record<string, string> {
  return Object.fromEntries(rows.map((row) => [row.key.trim(), row.value]))
}

function parameterRowsObject(
  rows: ParameterRow[],
): AdminChannelCreateRequestWritable['param_override'] {
  const overrides: AdminChannelCreateRequestWritable['param_override'] = {}
  for (const row of rows) {
    switch (row.key) {
      case 'temperature':
        overrides.temperature = Number(row.value)
        break
      case 'top_p':
        overrides.top_p = Number(row.value)
        break
      case 'max_output_tokens':
        overrides.max_output_tokens = Number(row.value)
        break
      case 'stop_sequences':
        overrides.stop_sequences = [...row.values]
        break
      case '':
        break
    }
  }
  return overrides
}

function stringMapRows(value: Record<string, string>, prefix: string): KeyValueRow[] {
  return Object.entries(value).map(([key, rowValue], index) => ({
    id: `${prefix}-${index}`,
    key,
    value: rowValue,
  }))
}

function parameterRows(value: Record<string, unknown>): ParameterRow[] {
  return Object.entries(value).flatMap(([key, rowValue], index) => {
    if (!isChannelParameterName(key)) return []
    return [{
      id: `parameter-${index}`,
      key,
      value: key === 'stop_sequences' ? '' : String(rowValue),
      values: key === 'stop_sequences' && Array.isArray(rowValue)
        ? rowValue.filter((item): item is string => typeof item === 'string')
        : [],
    }]
  })
}

function validateStringRows(
    rows: KeyValueRow[],
  path: 'modelMapping' | 'responsesCompactModelMapping',
  messages: ValidationMessages,
  context: z.RefinementCtx,
) {
  if (rows.length > 512 || rows.some((row) => !isBoundedText(row.key, 255) || !isBoundedText(row.value, 255))) {
    addIssue(context, path, messages.invalidEntries)
  } else if (!uniqueKeys(rows)) {
    addIssue(context, path, messages.duplicateKey)
  }
}

function validateHeaderRows(
  rows: KeyValueRow[],
  messages: ValidationMessages,
  context: z.RefinementCtx,
) {
  const valid = rows.length <= 64 && rows.every((row) => {
    const name = row.key.trim().toLowerCase()
    return row.key.length <= 200
      && HEADER_NAME_PATTERN.test(row.key.trim())
      && !FORBIDDEN_HEADERS.has(name)
      && new TextEncoder().encode(row.value).length <= 8192
      && !/[^\t\x20-\x7e\x80-\xff]/u.test(row.value)
  })
  if (!valid) addIssue(context, 'headerOverride', messages.invalidHeader)
  else if (!uniqueKeys(rows, true)) addIssue(context, 'headerOverride', messages.duplicateKey)
}

function validateParameterRows(
  rows: ParameterRow[],
  protocol: ChannelProtocol,
  messages: ValidationMessages,
  context: z.RefinementCtx,
) {
  if (protocol === 'openai_embeddings' || protocol === 'openai_images' || protocol === 'openai_audio' || protocol === 'openai_speech' || protocol === 'jina_rerank' || protocol === 'cohere_rerank' || protocol === 'xai_video') {
    if (rows.length > 0) addIssue(context, 'paramOverride', messages.invalidParameter)
    return
  }
  const valid = rows.length <= channelParameterNames.length && rows.every((row) => {
    if (!isChannelParameterName(row.key)) return false
    if (row.key === 'stop_sequences') {
      return protocol !== 'openai_responses'
        && row.values.length >= 1
        && row.values.length <= 4
        && row.values.every((value) => value.length > 0 && new TextEncoder().encode(value).length <= 1024)
    }
    if (row.value.trim() === '' || !Number.isFinite(Number(row.value))) return false
    const value = Number(row.value)
    if (row.key === 'temperature') {
      return value >= 0 && value <= (protocol === 'anthropic' ? 1 : 2)
    }
    if (row.key === 'top_p') return value >= 0 && value <= 1
    return Number.isSafeInteger(value) && value >= 1 && value <= 1_000_000
  })
  if (!valid) addIssue(context, 'paramOverride', messages.invalidParameter)
  else if (!uniqueKeys(rows)) addIssue(context, 'paramOverride', messages.duplicateKey)
}

function uniqueKeys(rows: KeyValueRow[], caseInsensitive = false) {
  const keys = rows.map((row) => {
    const key = row.key.trim()
    return caseInsensitive ? key.toLowerCase() : key
  })
  return new Set(keys).size === keys.length
}

function uniqueStrings(values: string[]) {
  return new Set(values).size === values.length
}

function isBoundedText(value: string, maxBytes: number) {
  return value.trim() === value && value.length > 0 && new TextEncoder().encode(value).length <= maxBytes
}

function isSafeBaseUrl(value: string) {
  try {
    const url = new URL(value)
    return ['http:', 'https:'].includes(url.protocol)
      && !url.username && !url.password && !url.search && !url.hash
  } catch {
    return false
  }
}

function isValidOptionalTimeout(value: string) {
  if (value.trim() === '') return true
  const seconds = Number(value)
  return Number.isSafeInteger(seconds) && seconds >= 1 && seconds <= MAX_CHANNEL_TIMEOUT_SECONDS
}

function validAutoBanRules(statusCodes: number[], keywords: string[]) {
  const encoder = new TextEncoder()
  const totalRules = statusCodes.length + keywords.length
  const uniqueStatuses = new Set(statusCodes).size === statusCodes.length
  const normalizedKeywords = keywords.map((keyword) => keyword.toLowerCase())
  const uniqueKeywords = new Set(normalizedKeywords).size === keywords.length
  const validStatuses = statusCodes.every(
    (status) => Number.isSafeInteger(status) && status >= 500 && status <= 599,
  )
  const validKeywords = keywords.every((keyword) => (
    keyword.trim() === keyword
    && keyword.length > 0
    && encoder.encode(keyword).length <= 256
    && !/[\p{Cc}\p{Cf}]/u.test(keyword)
  ))
  const keywordBytes = keywords.reduce((total, keyword) => total + encoder.encode(keyword.toLowerCase()).length, 0)
  return totalRules <= 64
    && uniqueStatuses
    && uniqueKeywords
    && validStatuses
    && validKeywords
    && keywordBytes <= 8 * 1024
}

function createRowId(prefix: string) {
  return `${prefix}-${globalThis.crypto.randomUUID()}`
}

function addIssue(
  context: z.RefinementCtx,
  path: keyof ChannelFormValues,
  message: string,
) {
  context.addIssue({ code: 'custom', message, path: [path] })
}

const HEADER_NAME_PATTERN = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/
const FORBIDDEN_HEADERS = new Set([
  'accept',
  'authorization',
  'content-type',
  'anthropic-version',
  'proxy-authorization',
  'x-api-key',
  'api-key',
  'x-goog-api-key',
  'x-auth-token',
  'x-access-token',
  'x-client-secret',
  'x-amz-security-token',
  'cookie',
  'host',
  'content-length',
  'connection',
  'proxy-connection',
  'keep-alive',
  'transfer-encoding',
  'upgrade',
  'te',
  'trailer',
  'x-request-id',
])

function isChannelParameterName(value: string): value is ChannelParameterName {
  return channelParameterNames.includes(value as ChannelParameterName)
}
