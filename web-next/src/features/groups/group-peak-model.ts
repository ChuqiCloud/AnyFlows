import type { AdminGroup, AdminGroupPeakWindow } from '@/lib/api/generated/types.gen'

export type PeakRuleForm = {
  weekdays: number[]
  start: string
  end: string
  ratio: string
  modelScope: 'all' | 'selected'
  models: string
  utcOffset: string
}

export const MAX_PEAK_RULES = 32
const DAY = 86_400
const WEEK = 7 * DAY
const TIME = /^(?:[01][0-9]|2[0-3]):[0-5][0-9](?::[0-5][0-9])?$/

export function newPeakRule(): PeakRuleForm {
  return { weekdays: [1, 2, 3, 4, 5, 6, 7], start: '08:00', end: '20:00', ratio: '2', modelScope: 'all', models: '', utcOffset: '480' }
}

export function timeSecond(value: string, allowDayEnd = false): number | undefined {
  if (allowDayEnd && (value === '24:00' || value === '24:00:00')) return DAY
  if (!TIME.test(value)) return undefined
  const [hours = 0, minutes = 0, seconds = 0] = value.split(':').map(Number)
  return hours * 3600 + minutes * 60 + seconds
}

export function secondTime(value: number): string {
  const hours = Math.floor(value / 3600)
  const minutes = Math.floor(value % 3600 / 60)
  const seconds = value % 60
  return [hours, minutes, seconds].map((part) => String(part).padStart(2, '0')).join(':')
}

export function ruleModels(value: string): string[] {
  return value.split(/[,，\r\n]+/u).map((name) => name.trim()).filter(Boolean)
}

export function groupPeakWindows(group?: AdminGroup): AdminGroupPeakWindow[] {
  if (group?.peak_windows != null) return group.peak_windows
  if (group?.peak_ratio_micros == null) return []
  const start = timeSecond(group.peak_start ?? '')
  const end = timeSecond(group.peak_end ?? '')
  if (start === undefined || end === undefined) return []
  return [{ weekdays: [1, 2, 3, 4, 5, 6, 7], start_second: start, end_second: end,
    ratio_micros: group.peak_ratio_micros, models: [], utc_offset_minutes: 0 }]
}

export function utcLabel(minutes: number): string {
  if (minutes === 0) return 'UTC'
  const absolute = Math.abs(minutes)
  return `UTC${minutes < 0 ? '-' : '+'}${String(Math.floor(absolute / 60)).padStart(2, '0')}:${String(absolute % 60).padStart(2, '0')}`
}

export function utcOptions(current: string): { key: string; label: string }[] {
  const offsets = Array.from({ length: 105 }, (_, index) => index * 15 - 720)
  const value = Number(current)
  if (Number.isInteger(value) && value >= -720 && value <= 840 && !offsets.includes(value)) offsets.push(value)
  return offsets.sort((a, b) => a - b).map((offset) => ({ key: String(offset), label: utcLabel(offset) }))
}

/** 将跨午夜、时区和周界统一展开，供保存前检测共同模型的冲突。 */
export function peakRulesOverlap(a: PeakRuleForm, b: PeakRuleForm): boolean {
  if (a.modelScope === 'selected' && b.modelScope === 'selected'
    && !ruleModels(a.models).some((model) => ruleModels(b.models).includes(model))) return false
  return intervals(a).some(([start, end]) => intervals(b).some(([otherStart, otherEnd]) => start < otherEnd && otherStart < end))
}

function intervals(rule: PeakRuleForm): [number, number][] {
  const start = timeSecond(rule.start)
  const end = timeSecond(rule.end, true)
  const offset = Number(rule.utcOffset)
  if (start === undefined || end === undefined || !Number.isInteger(offset)) return []
  const duration = end > start ? end - start : DAY - start + end
  return rule.weekdays.flatMap((day): [number, number][] => {
    const from = (((day - 1) * DAY + start - offset * 60) % WEEK + WEEK) % WEEK
    const to = from + duration
    return to > WEEK ? [[from, WEEK], [0, to - WEEK]] : [[from, to]]
  })
}

