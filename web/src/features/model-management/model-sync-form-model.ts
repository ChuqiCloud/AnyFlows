import { z } from 'zod'

import type {
  AdminModelModality,
  AdminModelSyncApplyItemRequest,
  AdminModelSyncPreviewItem,
} from '@/lib/api/generated/types.gen'
import {
  optionalModelText,
  validModelContextWindow,
  validModelTags,
  validModelText,
  validOptionalModelIconUrl,
  validOptionalModelText,
  type ModelValidationMessages,
} from './model-management-form-model.ts'

export type ModelSyncDraft = {
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
}

const modalitySchema = z.enum(['text', 'image', 'audio', 'video'])

/** 校验管理员明确确认的权威字段，不把上游证据自动视为事实。 */
export function buildModelSyncDraftSchema(messages: ModelValidationMessages) {
  return z.object({
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
  })
}

const validationOnlyMessages: ModelValidationMessages = {
  model: '',
  displayName: '',
  provider: '',
  description: '',
  iconUrl: '',
  tags: '',
  contextWindow: '',
  modalities: '',
}

export function modelSyncDraftIsComplete(draft?: ModelSyncDraft) {
  return draft !== undefined
    && buildModelSyncDraftSchema(validationOnlyMessages).safeParse(draft).success
}

export function defaultModelSyncDraft(): ModelSyncDraft {
  return {
    displayName: '',
    provider: '',
    description: '',
    iconUrl: '',
    tags: [],
    contextWindow: '',
    inputModalities: [],
    outputModalities: [],
    supportsReasoning: false,
    supportsToolCalls: false,
  }
}

export function toModelSyncApplyItem(
  item: AdminModelSyncPreviewItem,
  draft: ModelSyncDraft,
): AdminModelSyncApplyItemRequest {
  return {
    item_id: item.item_id,
    display_name: draft.displayName.trim(),
    provider: draft.provider.trim(),
    description: optionalModelText(draft.description),
    icon_url: optionalModelText(draft.iconUrl),
    tags: draft.tags,
    context_window: draft.contextWindow ? Number(draft.contextWindow) : null,
    input_modalities: draft.inputModalities,
    output_modalities: draft.outputModalities,
    supports_reasoning: draft.supportsReasoning,
    supports_tool_calls: draft.supportsToolCalls,
  }
}
