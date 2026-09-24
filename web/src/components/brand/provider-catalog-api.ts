import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { providerCatalog, type ProviderOption } from './provider-catalog'

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
  const normalized = provider.trim().toLowerCase()
  return options.find((item) => [item.id, item.name, ...(item.aliases ?? [])]
    .some((value) => value.toLowerCase() === normalized))
}

export function useModelProviderOptions() {
  const query = useQuery({
    queryKey: ['model-provider-catalog'],
    staleTime: 60_000,
    queryFn: async ({ signal }) => {
      const { data } = await apiClient.get<ModelProviderCatalogResponse>({ url: '/api/model-provider-catalog', signal, throwOnError: true })
      return data.providers
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
