import { z } from 'zod'

import type {
  AdminMissingModel,
  AdminMissingModelImportRequest,
  AdminModelModality,
} from '@/lib/api/generated/types.gen'
import {
  validModelText,
  type ModelValidationMessages,
} from './model-management-form-model.ts'

export type MissingModelImportDraft = {
  provider: string
  inputModalities: AdminModelModality[]
  outputModalities: AdminModelModality[]
  supportsReasoning: boolean
  supportsToolCalls: boolean
}

const modalitySchema = z.enum(['text', 'image', 'audio', 'video'])

/** 校验整批共享的权威字段，不从 Canonical 名称推断厂商或能力。 */
export function buildMissingModelImportSchema(messages: ModelValidationMessages) {
  return z.object({
    provider: z.string().refine((value) => validModelText(value, 64), messages.provider),
    inputModalities: z.array(modalitySchema).min(1, messages.modalities),
    outputModalities: z.array(modalitySchema).min(1, messages.modalities),
    supportsReasoning: z.boolean(),
    supportsToolCalls: z.boolean(),
  })
}

export function defaultMissingModelImportDraft(): MissingModelImportDraft {
  return {
    provider: '',
    inputModalities: [],
    outputModalities: [],
    supportsReasoning: false,
    supportsToolCalls: false,
  }
}

export function missingDisplayNamesAreValid(
  models: AdminMissingModel[],
  displayNames: Record<string, string>,
) {
  return models.every((model) => validModelText(displayNames[model.model] ?? model.model, 128))
}

/** 构造快速导入请求，只提交管理员明确确认的字段。 */
export function toMissingModelImportRequest(
  models: AdminMissingModel[],
  displayNames: Record<string, string>,
  draft: MissingModelImportDraft,
): AdminMissingModelImportRequest {
  return {
    items: models.map((model) => ({
      model: model.model,
      display_name: (displayNames[model.model] ?? model.model).trim(),
      provider: draft.provider.trim(),
      description: null,
      icon_url: null,
      tags: [],
      context_window: null,
      input_modalities: draft.inputModalities,
      output_modalities: draft.outputModalities,
      supports_reasoning: draft.supportsReasoning,
      supports_tool_calls: draft.supportsToolCalls,
    })),
  }
}
