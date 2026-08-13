import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import type { PlaygroundProtocol } from './playground-protocol'

export const ALL_PLAYGROUND_MODEL_FILTER = 'all' as const
export type PlaygroundModelProviderFilter = typeof ALL_PLAYGROUND_MODEL_FILTER | string
export type PlaygroundModelProtocolFilter = typeof ALL_PLAYGROUND_MODEL_FILTER | PlaygroundProtocol

export type PlaygroundModelFilter = {
  provider: PlaygroundModelProviderFilter
  protocol: PlaygroundModelProtocolFilter
}

export type PlaygroundModelProviderCategory = {
  count: number
  value: PlaygroundModelProviderFilter
}

export const DEFAULT_PLAYGROUND_MODEL_FILTER: PlaygroundModelFilter = {
  provider: ALL_PLAYGROUND_MODEL_FILTER,
  protocol: ALL_PLAYGROUND_MODEL_FILTER,
}

/** 从当前已加载的目录页生成供应商分类，分类数量与列表内容保持一致。 */
export function playgroundModelProviderCategories(
  models: readonly ModelCatalogItem[],
): PlaygroundModelProviderCategory[] {
  const counts = new Map<string, number>()
  for (const item of models) {
    const provider = item.provider.trim()
    if (!provider) continue
    counts.set(provider, (counts.get(provider) ?? 0) + 1)
  }
  return [
    { value: ALL_PLAYGROUND_MODEL_FILTER, count: models.length },
    ...[...counts]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([value, count]) => ({ value, count })),
  ]
}

/** 组合供应商与协议筛选，搜索词仍由目录接口负责。 */
export function filterPlaygroundModels(
  models: readonly ModelCatalogItem[],
  filter: PlaygroundModelFilter,
) {
  return models.filter((item) => (
    (filter.provider === ALL_PLAYGROUND_MODEL_FILTER || item.provider === filter.provider)
    && (
      filter.protocol === ALL_PLAYGROUND_MODEL_FILTER
      || item.available_protocols.includes(filter.protocol)
    )
  ))
}
