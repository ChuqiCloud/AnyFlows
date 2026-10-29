export type SelectableTokenGroup = {
  id: number
  display_name: string
  ratio_micros: number
  is_default: boolean
}

/** 空值跟随用户默认组；固定绑定必须来自完整、成功读取的授权目录。 */
export function isSelectableTokenGroup(value: string, groups?: SelectableTokenGroup[]) {
  if (!groups) return false
  if (value === '') return true
  const id = Number(value)
  return Number.isSafeInteger(id) && id > 0 && groups.some((group) => group.id === id)
}

export function formatGroupBaseRatio(ratioMicros: number) {
  return String(ratioMicros / 1_000_000)
}
