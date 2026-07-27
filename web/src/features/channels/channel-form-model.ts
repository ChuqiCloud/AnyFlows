import { z } from 'zod'

import type {
  AdminChannel,
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

export type ChannelEditorMode = 'create' | 'update'

export type ChannelFormValues = {
  name: string
  baseUrl: string
  tag: string
  status: 'enabled' | 'disabled'
  weight: number
  priority: number
  autoBan: boolean
  models: string
  groupIds: string
  modelMapping: string
  paramOverride: string
  headerOverride: string
  settings: string
}

type ValidationMessages = {
  invalidField: string
  invalidJson: string
  invalidStringMap: string
  invalidUrl: string
  invalidRouting: string
  sensitiveRequired: string
}

/** 把跨字段路由约束和创建时敏感字段要求集中到同一份表单 schema。 */
export function buildChannelFormSchema(mode: ChannelEditorMode, messages: ValidationMessages) {
  return z.object({
    name: z.string().trim().min(1, messages.invalidField).max(128, messages.invalidField),
    baseUrl: z.string().trim().max(2048, messages.invalidField),
    tag: z.string().trim().max(64, messages.invalidField),
    status: z.enum(['enabled', 'disabled']),
    weight: z.number().int(messages.invalidField).min(0, messages.invalidField),
    priority: z.number().int(messages.invalidField),
    autoBan: z.boolean(),
    models: z.string(),
    groupIds: z.string(),
    modelMapping: z.string().refine(isStringMap, messages.invalidStringMap),
    paramOverride: z.string().refine(isJsonObject, messages.invalidJson),
    headerOverride: z.string().refine((value) => !value.trim() || isStringMap(value), messages.invalidStringMap),
    settings: z.string().refine((value) => !value.trim() || isJsonObject(value), messages.invalidJson),
  }).superRefine((values, context) => {
    if (values.baseUrl && !isSafeBaseUrl(values.baseUrl)) {
      context.addIssue({ code: 'custom', message: messages.invalidUrl, path: ['baseUrl'] })
    }

    const models = splitList(values.models)
    const groups = parseGroupIds(values.groupIds)
    const routingValid = groups !== undefined
      && (models.length === 0) === (groups.length === 0)
      && models.length <= 512
      && models.every((model) => new TextEncoder().encode(model).length <= 255)
      && groups.length <= 64
      && models.length * groups.length <= 4096
    if (!routingValid) {
      context.addIssue({ code: 'custom', message: messages.invalidRouting, path: ['groupIds'] })
    }

    if (mode === 'create') {
      for (const field of ['headerOverride', 'settings'] as const) {
        if (!values[field].trim()) {
          context.addIssue({ code: 'custom', message: messages.sensitiveRequired, path: [field] })
        }
      }
    }
  })
}

export function defaultChannelValues(channel?: AdminChannel): ChannelFormValues {
  return {
    name: channel?.name ?? '',
    baseUrl: channel?.base_url ?? '',
    tag: channel?.tag ?? '',
    status: channel?.status === 'enabled' ? 'enabled' : 'disabled',
    weight: channel?.weight ?? 10,
    priority: channel?.priority ?? 0,
    autoBan: channel?.auto_ban ?? true,
    models: channel?.models.join('\n') ?? '',
    groupIds: channel?.group_ids.join(', ') ?? '',
    modelMapping: formatJson(channel?.model_mapping ?? {}),
    paramOverride: formatJson(channel?.param_override ?? {}),
    headerOverride: channel ? '' : '{}',
    settings: channel ? '' : '{}',
  }
}

export function toCreateRequest(values: ChannelFormValues): AdminChannelCreateRequestWritable {
  return {
    ...commonRequest(values),
    header_override: parseJsonObject(values.headerOverride) as Record<string, string>,
    settings: parseJsonObject(values.settings),
  }
}

export function toUpdateRequest(values: ChannelFormValues): AdminChannelUpdateRequestWritable {
  return {
    ...commonRequest(values),
    ...(values.headerOverride.trim() && { header_override: parseJsonObject(values.headerOverride) as Record<string, string> }),
    ...(values.settings.trim() && { settings: parseJsonObject(values.settings) }),
  }
}

function commonRequest(values: ChannelFormValues) {
  return {
    name: values.name.trim(),
    type: 'openai' as const,
    protocol: 'openai_chat' as const,
    base_url: values.baseUrl.trim() || null,
    status: values.status,
    weight: values.weight,
    priority: values.priority,
    auto_ban: values.autoBan,
    models: splitList(values.models),
    group_ids: parseGroupIds(values.groupIds) ?? [],
    model_mapping: parseJsonObject(values.modelMapping) as Record<string, string>,
    param_override: parseJsonObject(values.paramOverride),
    tag: values.tag.trim() || null,
  }
}

function splitList(value: string) {
  return [...new Set(value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean))]
}

function parseGroupIds(value: string) {
  const items = splitList(value)
  const ids = items.map(Number)
  return ids.every((id) => Number.isSafeInteger(id) && id > 0) ? ids : undefined
}

function isJsonObject(value: string) {
  try {
    const parsed: unknown = JSON.parse(value)
    return typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
  } catch {
    return false
  }
}

function isStringMap(value: string) {
  if (!isJsonObject(value)) return false
  return Object.values(parseJsonObject(value)).every((item) => typeof item === 'string')
}

function parseJsonObject(value: string): Record<string, unknown> {
  return JSON.parse(value) as Record<string, unknown>
}

function formatJson(value: unknown) {
  return JSON.stringify(value, null, 2)
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
