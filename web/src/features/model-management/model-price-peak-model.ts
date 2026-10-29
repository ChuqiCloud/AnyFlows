import type { AdminGroupPeakWindow } from '@/lib/api/generated/types.gen'

import { ratioFromMicros, ratioToMicros } from '../groups/group-form-model.ts'
import { peakRulesOverlap, secondTime, timeSecond, type PeakRuleForm } from '../groups/group-peak-model.ts'

export type ModelPeakRuleForm = Pick<PeakRuleForm, 'weekdays' | 'start' | 'end' | 'ratio' | 'utcOffset'>

export const MAX_MODEL_PEAK_RULES = 32

export function modelPeakRules(windows?: AdminGroupPeakWindow[] | null): ModelPeakRuleForm[] {
  return (windows ?? []).map((window) => ({
    weekdays: window.weekdays,
    start: secondTime(window.start_second),
    end: secondTime(window.end_second),
    ratio: ratioFromMicros(window.ratio_micros),
    utcOffset: String(window.utc_offset_minutes ?? 0),
  }))
}

export function modelPeakRulesOverlap(a: ModelPeakRuleForm, b: ModelPeakRuleForm): boolean {
  const groupRule = (rule: ModelPeakRuleForm): PeakRuleForm => ({
    ...rule,
    modelScope: 'all',
    models: '',
  })
  return peakRulesOverlap(groupRule(a), groupRule(b))
}

export function modelPeakRuleWindows(rules: ModelPeakRuleForm[]): AdminGroupPeakWindow[] {
  return rules.map((rule) => ({
    weekdays: rule.weekdays,
    start_second: timeSecond(rule.start)!,
    end_second: timeSecond(rule.end, true)!,
    ratio_micros: ratioToMicros(rule.ratio)!,
    models: [],
    utc_offset_minutes: Number(rule.utcOffset),
  }))
}

export function newModelPeakRule(): ModelPeakRuleForm {
  return { weekdays: [1, 2, 3, 4, 5, 6, 7], start: '08:00', end: '20:00', ratio: '2', utcOffset: '480' }
}
