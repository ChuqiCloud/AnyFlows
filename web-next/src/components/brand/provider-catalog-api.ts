import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { normalizeProvider, providerCatalog, type ProviderOption } from './provider-catalog'

export type ModelProviderCatalogEntry = {
  provider_key: string
  display_name: string
  logo: string | null
  aliases: string[]
  enabled: boolean
  sort_order: number
  version: number
}

type ModelProviderCatalogResponse = { providers: ModelProviderCatalogEntry[] }

export function findProviderOption(provider: string, options: readonly ProviderOption[]) {
  const normalized = normalizeProvider(provider)
  return options
    .map((item) => ({ item, score: [item.id, item.name, ...(item.aliases ?? [])].reduce((best, value) => {
      const candidate = normalizeProvider(value)
      if (candidate === normalized) return Math.max(best, candidate.length + 1000)
      return candidate.length >= 3 && normalized.startsWith(candidate) ? Math.max(best, candidate.length) : best
    }, 0) }))
    .filter(({ score }) => score > 0)
    .sort((left, right) => right.score - left.score)[0]?.item
}

export function useModelProviderOptions() {
  const query = useQuery({
    queryKey: ['model-provider-catalog'],
    staleTime: 60_000,
    queryFn: async ({ signal }) => {
      const result = await apiClient.get({ url: '/api/model-provider-catalog', signal }) as unknown as { data: ModelProviderCatalogResponse }
      return result.data.providers
    },
  })
  const configured = new Map((query.data ?? []).filter((provider) => provider.provider_key.trim()).map((provider): [string, ModelProviderCatalogEntry] => [provider.provider_key, provider]))
  const known = new Set(providerCatalog.map((provider) => provider.id))
  const builtIns = providerCatalog
    .map((provider): ProviderOption | null => {
      const override = configured.get(provider.id)
      if (override?.enabled === false) return null
      return override ? { ...provider, name: override.display_name, logo: override.logo ?? provider.logo, aliases: [...new Set([...(provider.aliases ?? []), ...override.aliases])] } : provider
    })
    .filter((provider): provider is ProviderOption => provider !== null)
  const custom = [...configured.values()]
    .filter((provider) => provider.enabled && !known.has(provider.provider_key))
    .map((provider): ProviderOption => ({ id: provider.provider_key, name: provider.display_name, logo: provider.logo ?? provider.provider_key, aliases: provider.aliases }))
  const options = [...builtIns, ...custom]
  return { ...query, options }
}
