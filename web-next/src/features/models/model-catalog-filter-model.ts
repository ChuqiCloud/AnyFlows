import type {
  ModelCatalogBillingMode,
  ModelCatalogCapability,
  ModelCatalogModality,
  ModelCatalogProtocol,
} from '@/lib/api/generated/types.gen'

export type ModelBillingFilter = 'all' | ModelCatalogBillingMode

export type ModelCatalogFilters = {
  billingMode: ModelBillingFilter
  providers: string[]
  inputModalities: ModelCatalogModality[]
  outputModalities: ModelCatalogModality[]
  capabilities: ModelCatalogCapability[]
  protocols: ModelCatalogProtocol[]
}

export const EMPTY_MODEL_CATALOG_FILTERS: ModelCatalogFilters = {
  billingMode: 'all',
  providers: [],
  inputModalities: [],
  outputModalities: [],
  capabilities: [],
  protocols: [],
}

/** 统计当前已启用的筛选条件，供桌面、移动端与空状态复用。 */
export function countModelCatalogFilters(filters: ModelCatalogFilters) {
  return (filters.billingMode === 'all' ? 0 : 1)
    + filters.providers.length
    + filters.inputModalities.length
    + filters.outputModalities.length
    + filters.capabilities.length
    + filters.protocols.length
}

/** 切换多选筛选值并保持稳定排序，避免查询缓存身份随操作顺序变化。 */
export function toggleModelCatalogFilter<T extends string>(
  values: T[],
  value: T,
  checked: boolean,
) {
  return checked
    ? [...values, value].sort()
    : values.filter((item) => item !== value)
}
