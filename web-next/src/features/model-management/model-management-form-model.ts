import { z } from 'zod'

import type {
  AdminModel,
  AdminModelCreateRequest,
  AdminModelLifecycle,
  AdminModelModality,
  AdminModelUpdateRequest,
  AdminModelVisibility,
} from '@/lib/api/generated/types.gen'

export type ModelEditorMode = 'create' | 'update'

export type ModelManagementFormValues = {
  model: string
  displayName: string
  provider: string
  description: string
  iconUrl: string
  tags: string[]
  contextWindow: string
  inputModalities: AdminModelModality[]
  outputModalities: AdminModelModality[]
  supportsReasoning: boolean
  supportsToolCalls: boolean
  visibility: AdminModelVisibility
  lifecycle: AdminModelLifecycle
}

export type ModelValidationMessages = {
  model: string
  displayName: string
  provider: string
  description: string
  iconUrl: string
  tags: string
  contextWindow: string
  modalities: string
}

const modalitySchema = z.enum(['text', 'image', 'audio', 'video'])

/** 生成与当前界面语言一致的结构化模型元数据表单校验器。 */
export function buildModelManagementSchema(messages: ModelValidationMessages) {
  return z.object({
    model: z.string().refine((value) => validModelText(value, 256), messages.model),
    displayName: z.string().refine((value) => validModelText(value, 128), messages.displayName),
    provider: z.string().refine((value) => validModelText(value, 64), messages.provider),
    description: z.string().refine((value) => validOptionalModelText(value, 4096), messages.description),
    iconUrl: z.string().refine(validOptionalModelIconUrl, messages.iconUrl),
    tags: z.array(z.string()).max(32, messages.tags).refine(validModelTags, messages.tags),
    contextWindow: z.string().refine(validModelContextWindow, messages.contextWindow),
    inputModalities: z.array(modalitySchema).min(1, messages.modalities),
    outputModalities: z.array(modalitySchema).min(1, messages.modalities),
    supportsReasoning: z.boolean(),
    supportsToolCalls: z.boolean(),
    visibility: z.enum(['public', 'authenticated', 'hidden']),
    lifecycle: z.enum(['draft', 'active', 'deprecated', 'retired']),
  })
}

export function defaultModelManagementValues(model?: AdminModel): ModelManagementFormValues {
  return {
    model: model?.model ?? '',
    displayName: model?.display_name ?? '',
    provider: model?.provider ?? 'openai',
    description: model?.description ?? '',
    iconUrl: model?.icon_url ?? '',
    tags: model?.tags ?? [],
    contextWindow: model?.context_window === null || model?.context_window === undefined
      ? ''
      : String(model.context_window),
    inputModalities: model?.input_modalities ?? ['text'],
    outputModalities: model?.output_modalities ?? ['text'],
    supportsReasoning: model?.supports_reasoning ?? false,
    supportsToolCalls: model?.supports_tool_calls ?? false,
    visibility: model?.visibility ?? 'hidden',
    lifecycle: model?.lifecycle ?? 'draft',
  }
}

export function toModelCreateRequest(values: ModelManagementFormValues): AdminModelCreateRequest {
  return {
    model: values.model.trim(),
    ...toMutableRequest(values),
  }
}

export function toModelUpdateRequest(values: ModelManagementFormValues): AdminModelUpdateRequest {
  return toMutableRequest(values)
}

function toMutableRequest(values: ModelManagementFormValues): AdminModelUpdateRequest {
  return {
    display_name: values.displayName.trim(),
    provider: values.provider.trim(),
    description: optionalModelText(values.description),
    icon_url: optionalModelText(values.iconUrl),
    tags: values.tags,
    context_window: values.contextWindow ? Number(values.contextWindow) : null,
    input_modalities: values.inputModalities,
    output_modalities: values.outputModalities,
    supports_reasoning: values.supportsReasoning,
    supports_tool_calls: values.supportsToolCalls,
    visibility: values.visibility,
    lifecycle: values.lifecycle,
  }
}

export function optionalModelText(value: string) {
  const normalized = value.trim()
  return normalized || null
}

export function validModelText(value: string, maximumBytes: number) {
  const normalized = value.trim()
  return normalized.length > 0
    && byteLength(normalized) <= maximumBytes
    && ![...normalized].some((character) => /\p{Cc}/u.test(character))
}

export function validOptionalModelText(value: string, maximumBytes: number) {
  return value.trim().length === 0 || validModelText(value, maximumBytes)
}

export function validOptionalModelIconUrl(value: string) {
  const normalized = value.trim()
  if (!normalized) return true
  if (byteLength(normalized) > 2048 || [...normalized].some((character) => /\p{Cc}/u.test(character))) {
    return false
  }
  if (normalized.startsWith('/') && !normalized.startsWith('//')) {
    return !normalized.includes('?') && !normalized.includes('#')
  }
  try {
    const url = new URL(normalized)
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && !url.username
      && !url.password
      && !url.hash
  } catch {
    return false
  }
}

export function validModelTags(tags: string[]) {
  return new Set(tags).size === tags.length
    && tags.every((tag) => validModelText(tag, 64))
    && byteLength(JSON.stringify(tags)) <= 8192
}

export function validModelContextWindow(value: string) {
  if (!value) return true
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= 1 && parsed <= 2_147_483_647
}

function byteLength(value: string) {
  return new TextEncoder().encode(value).length
}
