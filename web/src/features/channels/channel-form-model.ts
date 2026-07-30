import { z } from 'zod'

import type {
  AdminChannel,
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

export type ChannelEditorMode = 'create' | 'update'
export type SensitiveEditMode = 'preserve' | 'replace' | 'clear'
export type ChannelProtocol = 'openai_chat' | 'openai_responses'

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
  name: string
  baseUrl: string
  protocol: ChannelProtocol
  tag: string
  status: 'enabled' | 'disabled'
  weight: number
  priority: number
  autoBan: boolean
  models: string[]
  groupIds: number[]
  modelMapping: KeyValueRow[]
  paramOverride: ParameterRow[]
  headerOverrideMode: SensitiveEditMode
  headerOverride: KeyValueRow[]
  settingsMode: SensitiveEditMode
  settings: string
}

type ValidationMessages = {
  duplicateKey: string
  invalidEntries: string
  invalidField: string
  invalidHeader: string
  invalidJson: string
  invalidParameter: string
  invalidRouting: string
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
export function buildChannelFormSchema(mode: ChannelEditorMode, messages: ValidationMessages) {
  return z.object({
    name: z.string().trim().min(1, messages.invalidField).max(128, messages.invalidField),
    baseUrl: z.string().trim().max(2048, messages.invalidField),
    protocol: z.enum(['openai_chat', 'openai_responses']),
    tag: z.string().trim().max(64, messages.invalidField),
    status: z.enum(['enabled', 'disabled']),
    weight: z.number().int(messages.invalidField).min(0, messages.invalidField),
    priority: z.number().int(messages.invalidField),
    autoBan: z.boolean(),
    models: z.array(z.string()),
    groupIds: z.array(z.number()),
    modelMapping: z.array(keyValueRowSchema),
    paramOverride: z.array(parameterRowSchema),
    headerOverrideMode: z.enum(['preserve', 'replace', 'clear']),
    headerOverride: z.array(keyValueRowSchema),
    settingsMode: z.enum(['preserve', 'replace', 'clear']),
    settings: z.string(),
  }).superRefine((values, context) => {
    if (values.baseUrl && !isSafeBaseUrl(values.baseUrl)) {
      addIssue(context, 'baseUrl', messages.invalidUrl)
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
    validateParameterRows(values.paramOverride, values.protocol, messages, context)
    if (mode === 'create' || values.headerOverrideMode === 'replace') {
      validateHeaderRows(values.headerOverride, messages, context)
    }
    if (values.settingsMode === 'replace' && !isJsonObject(values.settings)) {
      addIssue(context, 'settings', messages.invalidJson)
    }
  })
}

export function defaultChannelValues(channel?: AdminChannel): ChannelFormValues {
  return {
    name: channel?.name ?? '',
    baseUrl: channel?.base_url ?? '',
    protocol: channel?.protocol === 'openai_responses' ? 'openai_responses' : 'openai_chat',
    tag: channel?.tag ?? '',
    status: channel?.status === 'enabled' ? 'enabled' : 'disabled',
    weight: channel?.weight ?? 10,
    priority: channel?.priority ?? 0,
    autoBan: channel?.auto_ban ?? true,
    models: [...(channel?.models ?? [])],
    groupIds: [...(channel?.group_ids ?? [])],
    modelMapping: stringMapRows(channel?.model_mapping ?? {}, 'mapping'),
    paramOverride: parameterRows(channel?.param_override ?? {}),
    headerOverrideMode: channel ? 'preserve' : 'replace',
    headerOverride: [],
    settingsMode: channel ? 'preserve' : 'replace',
    settings: '{}',
  }
}

export function toCreateRequest(values: ChannelFormValues): AdminChannelCreateRequestWritable {
  return {
    ...commonRequest(values),
    header_override: stringRowsObject(values.headerOverride),
    settings: values.settingsMode === 'replace' ? parseJsonObject(values.settings) : {},
  }
}

export function toUpdateRequest(values: ChannelFormValues): AdminChannelUpdateRequestWritable {
  const headerOverride = sensitiveValue(
    values.headerOverrideMode,
    stringRowsObject(values.headerOverride),
  )
  const settings = sensitiveValue(
    values.settingsMode,
    values.settingsMode === 'replace' ? parseJsonObject(values.settings) : {},
  )
  return {
    ...commonRequest(values),
    ...(headerOverride !== undefined && { header_override: headerOverride }),
    ...(settings !== undefined && { settings }),
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
    name: values.name.trim(),
    type: 'openai' as const,
    protocol: values.protocol,
    base_url: values.baseUrl.trim() || null,
    status: values.status,
    weight: values.weight,
    priority: values.priority,
    auto_ban: values.autoBan,
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
  path: 'modelMapping',
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
  const valid = rows.length <= channelParameterNames.length && rows.every((row) => {
    if (!isChannelParameterName(row.key)) return false
    if (row.key === 'stop_sequences') {
      return protocol === 'openai_chat'
        && row.values.length >= 1
        && row.values.length <= 4
        && row.values.every((value) => value.length > 0 && new TextEncoder().encode(value).length <= 1024)
    }
    if (row.value.trim() === '' || !Number.isFinite(Number(row.value))) return false
    const value = Number(row.value)
    if (row.key === 'temperature') return value >= 0 && value <= 2
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

function isJsonObject(value: string) {
  try {
    const parsed: unknown = JSON.parse(value)
    return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
  } catch {
    return false
  }
}

function parseJsonObject(value: string): Record<string, unknown> {
  return JSON.parse(value) as Record<string, unknown>
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
