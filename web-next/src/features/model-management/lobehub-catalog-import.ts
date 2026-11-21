import { lobeHubProviders } from '@/components/brand/lobehub-providers'
import { providerCatalog } from '@/components/brand/provider-catalog'
import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminModel,
  listAdminModelProviderCatalog,
  listAdminModels,
  updateAdminModelProvider,
} from '@/lib/api/generated/sdk.gen'
import type { AdminModelCreateRequest, AdminModelModality } from '@/lib/api/generated/types.gen'

const IMPORT_CONCURRENCY = 4

export type LobeHubImportProgress = {
  total: number
  providersCreated: number
  modelsCreated: number
  skipped: number
}

type CatalogModel = {
  id: string
  name: string
  provider: string
  input: AdminModelModality[]
  output: AdminModelModality[]
  context: number | null
  type: string
  reasoning: boolean
  tools: boolean
}

function createRequest(model: CatalogModel): AdminModelCreateRequest {
  return {
    model: model.id,
    display_name: model.name,
    provider: model.provider,
    tags: ['lobehub', model.type],
    context_window: model.context,
    input_modalities: model.input,
    output_modalities: model.output,
    supports_reasoning: model.reasoning,
    supports_tool_calls: model.tools,
    visibility: 'hidden',
    lifecycle: 'draft',
  }
}

async function loadExistingModels(signal: AbortSignal) {
  const existing = new Set<string>()
  let cursor: number | undefined
  for (;;) {
    signal.throwIfAborted()
    const { data } = await listAdminModels({ client: apiClient, query: { after: cursor, limit: 100 }, signal })
    for (const model of data.models) existing.add(model.model)
    if (data.next_cursor === null) return existing
    if (data.next_cursor <= (cursor ?? 0)) throw new Error('Model catalog cursor did not advance')
    cursor = data.next_cursor
  }
}

export async function importLobeHubCatalog(
  signal: AbortSignal,
  onProgress: (progress: LobeHubImportProgress) => void,
) {
  const { default: snapshot } = await import('./lobehub-catalog.json')
  const models: readonly CatalogModel[] = snapshot.models as CatalogModel[]
  const [existingModels, { data: providerList }] = await Promise.all([
    loadExistingModels(signal),
    listAdminModelProviderCatalog({ client: apiClient, signal }),
  ])
  const existingProviders = new Set(providerList.providers.map(({ provider_key }) => provider_key))
  const providers = lobeHubProviders.filter(({ id }) => !existingProviders.has(id))
  const missingModels = models.filter(({ id }) => !existingModels.has(id))
  const options = new Map(providerCatalog.map((provider) => [provider.id, provider]))
  const progress: LobeHubImportProgress = {
    total: lobeHubProviders.length + models.length,
    providersCreated: 0,
    modelsCreated: 0,
    skipped: lobeHubProviders.length - providers.length + models.length - missingModels.length,
  }
  onProgress({ ...progress })

  async function importItems<T>(
    items: readonly T[],
    count: 'providersCreated' | 'modelsCreated',
    write: (item: T) => Promise<unknown>,
  ) {
    for (let start = 0; start < items.length; start += IMPORT_CONCURRENCY) {
      signal.throwIfAborted()
      const batch = await Promise.allSettled(items.slice(start, start + IMPORT_CONCURRENCY).map(write))
      let failure: PromiseRejectedResult | undefined
      for (const result of batch) {
        if (result.status === 'fulfilled') progress[count]++
        else if (result.reason instanceof ApiError && result.reason.status === 409) progress.skipped++
        else failure ??= result
      }
      onProgress({ ...progress })
      if (failure) throw failure.reason
    }
  }

  await importItems(providers, 'providersCreated', ({ id, name }) => {
    const option = options.get(id)
    return updateAdminModelProvider({
      client: apiClient,
      signal,
      path: { provider_key: id },
      body: {
        expected_version: 0,
        display_name: option?.name ?? name,
        logo: option?.logo ?? id,
        aliases: [...(option?.aliases ?? [])],
        enabled: true,
        sort_order: 100,
      },
    })
  })
  await importItems(missingModels, 'modelsCreated', (model) => createAdminModel({
    body: createRequest(model), client: apiClient, signal,
  }))
  return progress
}
