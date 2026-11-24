import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  approveAdminPluginArtifact,
  configureAdminPluginProviderSelection,
  configureAdminPluginConfiguration,
  deployAdminPluginArtifact,
  disableAdminPluginDeployment,
  fetchPluginMarketIndex,
  getAdminPluginConfiguration,
  listAdminPluginArtifacts,
  listAdminPluginDeployments,
  listAdminPluginDeploymentHistory,
  installAdminMarketPlugin,
  registerAdminVerifiedPluginArtifact,
  revokeAdminPluginArtifact,
  rollbackAdminPluginDeployment,
} from '@/lib/api/generated/sdk.gen'
import type {
  PluginArtifactRegistrationRequestWritable,
  PluginDeploymentResponse,
  PluginDeploymentRequestWritable,
  PluginMarketInstallHttpRequestWritable,
  PluginProviderSelectionRequest,
  PluginPublicConfigurationUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

const root = ['admin-plugin-lifecycle'] as const
const artifactsKey = [...root, 'artifacts'] as const
const deploymentsKey = [...root, 'deployments'] as const

export type PluginRuntimeNode = {
  nodeId: string
  hostLabel: string
  lastSeenAt: number
  live: boolean
}

export type PluginRuntimeState = {
  nodeId: string
  pluginId: string
  generation: number
  digest: string
  status: string
  isolation: string
  errorCode: string | null
  updatedAt: number
  nodeLastSeenAt: number
}

export type PluginRuntimeOverview = {
  localNodeId: string | null
  isolation: string
  nodes: PluginRuntimeNode[]
  states: PluginRuntimeState[]
}

export type PluginEventResult = {
  deliveryId: number
  pluginId: string
  eventKind: string
  deploymentGeneration: number
  capabilityId: string
  capabilityVersion: number
  attemptCount: number
  status: string
  completedAt: number
  summary: string
  result: Record<string, unknown>
}

type PluginEventResultPage = {
  results: PluginEventResult[]
  nextBeforeId: number | null
}

export type UnknownPluginTask = {
  id: string
  pluginId: string
  model: string
  state: string
  maxUnits: number
  failureCode: string | null
  createdAt: number
  updatedAt: number
}

type UnknownPluginTaskPage = {
  tasks: UnknownPluginTask[]
  nextCursor: string | null
}

export function useUnknownPluginTasks() {
  return useInfiniteQuery({
    queryKey: [...root, 'unknown-tasks'],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const response = await apiClient.get<UnknownPluginTaskPage>({
        url: '/api/admin/plugins/tasks/unknown',
        query: { cursor: pageParam, limit: 100 },
        signal,
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: UnknownPluginTaskPage }
      return response.data
    },
    getNextPageParam: (page) => page.nextCursor ?? undefined,
    refetchInterval: 15_000,
    staleTime: 10_000,
    retry: false,
  })
}

export function useReconcileUnknownPluginTask() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ taskId, actualUnits, reason }: { taskId: string; actualUnits: number; reason: string }) => {
      const response = await apiClient.post<UnknownPluginTask>({
        url: `/api/admin/plugins/tasks/${encodeURIComponent(taskId)}/reconcile`,
        body: { actualUnits, reason },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: UnknownPluginTask }
      return response.data
    },
    onSuccess: async () => {
      await invalidate(client)
      await client.invalidateQueries({ queryKey: [...root, 'unknown-tasks'] })
    },
  })
}

export function usePluginEventResults() {
  return useInfiniteQuery({
    queryKey: [...root, 'event-results'],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const response = await apiClient.get<PluginEventResultPage>({
        url: '/api/admin/plugins/events/results',
        query: { before_id: pageParam, limit: 20 },
        signal,
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: PluginEventResultPage }
      return response.data
    },
    getNextPageParam: (page) => page.nextBeforeId ?? undefined,
    staleTime: 15_000,
    retry: false,
  })
}

export function usePluginRuntimeOverview() {
  return useQuery({
    queryKey: [...root, 'runtime'],
    queryFn: async ({ signal }) => {
      const result = await apiClient.get<PluginRuntimeOverview>({
        url: '/api/admin/plugins/runtime', signal,
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: PluginRuntimeOverview }
      return result.data
    },
    refetchInterval: 15_000,
    retry: false,
  })
}

export function useUninstallPlugin() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, generation }: { pluginId: string; generation: number }) => {
      const result = await apiClient.post<{ pluginId: string; generation: number; pendingNodes: number }>({
        url: `/api/admin/plugins/${encodeURIComponent(pluginId)}/uninstall`,
        body: { expected_generation: generation },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: { pluginId: string; generation: number; pendingNodes: number } }
      return result.data
    },
    onSuccess: () => invalidate(client),
  })
}

export function useCleanupPluginArtifact() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (artifactId: number) => {
      const result = await apiClient.post<{ artifactId: number; archiveRemoved: boolean; localFilesRemoved: boolean }>({
        url: `/api/admin/plugins/artifacts/${artifactId}/cleanup`,
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: { artifactId: number; archiveRemoved: boolean; localFilesRemoved: boolean } }
      return result.data
    },
    onSuccess: () => invalidate(client),
  })
}

export type PluginCatalogSource = {
  pluginId: string
  displayName: string
  pluginVersion: string
  generation: number
  supportsSearch: boolean
}

export type PluginCatalogModel = {
  id: string
  displayName: string
  provider: string
  description: string | null
  contextWindow: number | null
  inputModalities: string[]
  outputModalities: string[]
  tags: string[]
}

export type PluginMarketIndex = {
  format: number
  title: string
  description?: string | null
  plugins: Array<{
    id: string
    title: string
    description?: string | null
    icon?: string | null
    author?: string | null
    repository?: string | null
    versions: Array<{
      version: string
      archiveUrl: string
      sha256: string
      source?: {
        kind: string
        archiveUrl: string
        sha256: string
        repository?: string | null
        commit?: string | null
        path?: string | null
      } | null
      commit?: string | null
      engines?: { anyflows?: string | null } | null
      capabilities: Array<string>
      publishedAt?: string | null
      changelog?: string | null
      minHostVersion?: string | null
      maxHostVersion?: string | null
      runtime?: string | null
    }>
  }>
}

export function usePluginCatalogSources(enabled = true) {
  return useInfiniteQuery({
    queryKey: ['plugin-model-catalog', 'sources'],
    enabled,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const result = await apiClient.get<{ sources: PluginCatalogSource[]; nextCursor: string | null }>({
        url: '/api/account/plugin-models/sources', signal,
        query: { cursor: pageParam },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: { sources: PluginCatalogSource[]; nextCursor: string | null } }
      return result.data
    },
    getNextPageParam: (page) => page.nextCursor ?? undefined,
    staleTime: 30_000,
    retry: false,
  })
}

export function usePluginMarketIndex(indexUrl: string) {
  return useQuery({
    queryKey: ['plugin-market', 'index', indexUrl],
    enabled: Boolean(indexUrl),
    queryFn: async ({ signal }) => {
      const { data } = await fetchPluginMarketIndex({
        client: apiClient,
        query: { url: indexUrl },
        signal,
      })
      return data as PluginMarketIndex
    },
    staleTime: 60_000,
    retry: false,
  })
}

export function usePluginCatalogModels(pluginId: string | undefined, search: string) {
  return useInfiniteQuery({
    queryKey: ['plugin-model-catalog', 'models', pluginId, search],
    enabled: Boolean(pluginId),
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const result = await apiClient.get<{ source: PluginCatalogSource; models: PluginCatalogModel[]; nextCursor: string | null }>({
        url: `/api/account/plugin-models/${encodeURIComponent(pluginId ?? '')}`, signal,
        query: { q: search || undefined, cursor: pageParam, limit: 24 },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: { source: PluginCatalogSource; models: PluginCatalogModel[]; nextCursor: string | null } }
      return result.data
    },
    getNextPageParam: (page) => page.nextCursor ?? undefined,
    staleTime: 15_000,
    retry: false,
  })
}

export function usePluginArtifacts() {
  return useInfiniteQuery({
    queryKey: artifactsKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminPluginArtifacts({ client: apiClient, query: { before_id: pageParam, limit: 50 }, signal })
      return data
    },
    getNextPageParam: (lastPage) => lastPage.nextBeforeId ?? undefined,
    retry: false,
  })
}

export function usePluginDeployments() {
  return useInfiniteQuery({
    queryKey: deploymentsKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminPluginDeployments({ client: apiClient, query: { cursor: pageParam, limit: 50 }, signal })
      return data
    },
    getNextPageParam: (lastPage) => lastPage.nextCursor ?? undefined,
    retry: false,
  })
}

export function usePluginDeploymentHistory(pluginId: string | undefined, refreshVersion = 0) {
  return useQuery({
    queryKey: [...root, 'history', pluginId, refreshVersion],
    enabled: Boolean(pluginId),
    queryFn: async ({ signal }) => {
      if (!pluginId) return []
      const { data } = await listAdminPluginDeploymentHistory({ client: apiClient, path: { plugin_id: pluginId }, query: { limit: 50 }, signal })
      return data
    },
    retry: false,
    staleTime: 0,
    refetchOnMount: 'always',
  })
}

export function usePluginPublicConfiguration(pluginId: string | undefined, enabled: boolean) {
  return useQuery({
    queryKey: [...root, 'configuration', pluginId],
    enabled: Boolean(pluginId) && enabled,
    queryFn: async ({ signal }) => {
      if (!pluginId) throw new Error('pluginId is required')
      const { data } = await getAdminPluginConfiguration({ client: apiClient, path: { plugin_id: pluginId }, signal })
      return data
    },
    retry: false,
    staleTime: 0,
    refetchOnWindowFocus: false,
  })
}

export type PluginCapabilityInvocationResult = {
  pluginId: string
  deploymentGeneration: number
  capabilityId: string
  capabilityVersion: number
  output: unknown
}

export function usePluginCustomCapability() {
  return useMutation({
    mutationFn: async ({ pluginId, capabilityId, capabilityVersion, input }: {
      pluginId: string
      capabilityId: string
      capabilityVersion: number
      input: unknown
    }) => {
      const result = await apiClient.post<PluginCapabilityInvocationResult>({
        url: `/api/admin/plugins/${encodeURIComponent(pluginId)}/capabilities/${encodeURIComponent(capabilityId)}/invoke`,
        body: { capability_version: capabilityVersion, input },
        security: [
          { key: 'bearerAuth', scheme: 'bearer', type: 'http' },
          { key: 'personalAccessTokenBearer', scheme: 'bearer', type: 'http' },
        ],
      }) as unknown as { data: PluginCapabilityInvocationResult }
      return result.data
    },
  })
}

export type PluginCapabilityActionProposal = {
  proposalKey: string
  expiresAt: number
  status: string
}

export type PluginCapabilityActionResult = {
  proposalKey: string
  status: string
  outcomeCode?: string
}

export function usePluginCapabilityActionProposal() {
  return useMutation({
    mutationFn: async ({ pluginId, capabilityId, capabilityVersion, idempotencyKey, input }: {
      pluginId: string
      capabilityId: string
      capabilityVersion: number
      idempotencyKey: string
      input: unknown
    }) => {
      const result = await apiClient.post<PluginCapabilityActionProposal>({
        url: `/api/admin/plugins/${encodeURIComponent(pluginId)}/capability-actions`,
        body: {
          capability_id: capabilityId,
          capability_version: capabilityVersion,
          idempotency_key: idempotencyKey,
          input,
        },
        security: [
          { key: 'bearerAuth', scheme: 'bearer', type: 'http' },
          { key: 'personalAccessTokenBearer', scheme: 'bearer', type: 'http' },
        ],
      }) as unknown as { data: PluginCapabilityActionProposal }
      return result.data
    },
  })
}

export function useConfirmPluginCapabilityAction() {
  return useMutation({
    mutationFn: async ({ proposalKey, confirmationKey, input }: { proposalKey: string; confirmationKey: string; input: unknown }) => {
      const result = await apiClient.post<PluginCapabilityActionResult>({
        url: `/api/admin/plugins/capability-actions/${encodeURIComponent(proposalKey)}/confirm`,
        body: { confirmation_key: confirmationKey, input },
        security: [
          { key: 'bearerAuth', scheme: 'bearer', type: 'http' },
          { key: 'personalAccessTokenBearer', scheme: 'bearer', type: 'http' },
        ],
      }) as unknown as { data: PluginCapabilityActionResult }
      return result.data
    },
  })
}

function invalidate(client: ReturnType<typeof useQueryClient>) {
  return client.invalidateQueries({ queryKey: root })
}

type PluginInstallSource =
  | { packageFile: File }
  | { repository: string; assetName?: string }

export function useInstallPlugin() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ source, config }: { source: PluginInstallSource; config?: Record<string, unknown> }) => {
      const body = new FormData()
      if ('packageFile' in source) {
        body.append('package', source.packageFile, source.packageFile.name)
      } else {
        body.append('repository', source.repository)
        if (source.assetName) body.append('asset_name', source.assetName)
      }
      if (config) body.append('config', JSON.stringify(config))
      const result = await apiClient.post<PluginDeploymentResponse>({
        url: '/api/admin/plugins/install',
        body,
        bodySerializer: null,
        headers: { 'Content-Type': null },
        security: [
          { key: 'bearerAuth', scheme: 'bearer', type: 'http' },
          { key: 'personalAccessTokenBearer', scheme: 'bearer', type: 'http' },
        ],
      }) as unknown as { data: PluginDeploymentResponse }
      return result.data
    },
    onSuccess: () => invalidate(client),
    onError: () => invalidate(client),
  })
}

export function useInstallMarketPlugin() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (body: PluginMarketInstallHttpRequestWritable) => {
      const { data } = await installAdminMarketPlugin({ client: apiClient, body })
      return data
    },
    onSuccess: () => invalidate(client),
    onError: () => invalidate(client),
  })
}

export function useConfigurePluginProvider() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, body }: { pluginId: string; body: PluginProviderSelectionRequest }) => {
      const { data } = await configureAdminPluginProviderSelection({ client: apiClient, path: { plugin_id: pluginId }, body })
      return data
    },
    onSuccess: () => invalidate(client),
    onError: () => invalidate(client),
  })
}

export function useRegisterPluginArtifact() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (body: PluginArtifactRegistrationRequestWritable) => {
      const { data } = await registerAdminVerifiedPluginArtifact({ client: apiClient, body })
      return data
    },
    onSuccess: () => invalidate(client),
  })
}

export function useApprovePluginArtifact() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (artifactId: number) => approveAdminPluginArtifact({ client: apiClient, path: { artifact_id: artifactId } }),
    onSuccess: () => invalidate(client),
  })
}

export function useRevokePluginArtifact() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ artifactId, reason }: { artifactId: number; reason: string }) => revokeAdminPluginArtifact({
      client: apiClient,
      path: { artifact_id: artifactId },
      body: { reason },
    }),
    onSuccess: () => invalidate(client),
  })
}

export function useDeployPluginArtifact() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, body }: { pluginId: string; body: PluginDeploymentRequestWritable }) => deployAdminPluginArtifact({
      client: apiClient,
      path: { plugin_id: pluginId },
      body,
    }),
    onSuccess: () => invalidate(client),
  })
}

export function useUpdatePluginConfiguration() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, body }: {
      pluginId: string
      body: PluginPublicConfigurationUpdateRequestWritable
    }) => {
      const { data } = await configureAdminPluginConfiguration({
        client: apiClient,
        path: { plugin_id: pluginId },
        body,
      })
      return data
    },
    onSuccess: () => invalidate(client),
  })
}

export function useDisablePluginDeployment() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, generation }: { pluginId: string; generation: number }) => disableAdminPluginDeployment({
      client: apiClient,
      path: { plugin_id: pluginId },
      body: { expected_generation: generation },
    }),
    onSuccess: () => invalidate(client),
  })
}

export function useRollbackPluginDeployment() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ pluginId, generation, targetGeneration }: { pluginId: string; generation: number; targetGeneration: number }) => rollbackAdminPluginDeployment({
      client: apiClient,
      path: { plugin_id: pluginId },
      body: { expected_generation: generation, target_generation: targetGeneration },
    }),
    onSuccess: () => invalidate(client),
  })
}
