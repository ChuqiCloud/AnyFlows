import { MAX_PLAYGROUND_MODELS } from './playground-types.ts'

export const DEFAULT_PLAYGROUND_COMPARISON_ENABLED = false

/** 规范化比较模型集合，保持原顺序并限制重复项与并发上限。 */
export function normalizePlaygroundModels(models: string[]) {
  return [...new Set(models.filter((model) => model.length > 0))].slice(0, MAX_PLAYGROUND_MODELS)
}

/** 根据选择模式收敛模型集合；单选模式始终只保留首个模型。 */
export function modelsForPlaygroundSelectionMode(models: string[], comparisonEnabled: boolean) {
  const normalized = normalizePlaygroundModels(models)
  return comparisonEnabled ? normalized : normalized.slice(0, 1)
}

/** 切换比较模型；至少保留一个模型，且最多允许四个独立请求。 */
export function togglePlaygroundModel(models: string[], candidate: string) {
  const normalized = normalizePlaygroundModels(models)
  if (normalized.includes(candidate)) {
    return normalized.length === 1
      ? normalized
      : normalized.filter((model) => model !== candidate)
  }
  return normalized.length >= MAX_PLAYGROUND_MODELS
    ? normalized
    : [...normalized, candidate]
}

/** 选择一个模型；只有显式开启对比模式时才允许切换集合成员。 */
export function selectPlaygroundModel(
  models: string[],
  candidate: string,
  comparisonEnabled: boolean,
) {
  return comparisonEnabled
    ? togglePlaygroundModel(models, candidate)
    : modelsForPlaygroundSelectionMode([candidate], false)
}
