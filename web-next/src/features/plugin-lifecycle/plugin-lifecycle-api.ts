import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  approveAdminPluginArtifact,
  deployAdminPluginArtifact,
  disableAdminPluginDeployment,
  listAdminPluginArtifacts,
  listAdminPluginDeployments,
  listAdminPluginDeploymentHistory,
  registerAdminVerifiedPluginArtifact,
  revokeAdminPluginArtifact,
  rollbackAdminPluginDeployment,
} from '@/lib/api/generated/sdk.gen'
import type {
  PluginArtifactRegistrationRequestWritable,
  PluginDeploymentResponse,
  PluginDeploymentRequestWritable,
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

function invalidate(client: ReturnType<typeof useQueryClient>) {
  return client.invalidateQueries({ queryKey: root })
}

export function useInstallPlugin() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async ({ packageFile, config }: { packageFile: File; config: Record<string, unknown> }) => {
      const body = new FormData()
      body.append('package', packageFile, packageFile.name)
      body.append('config', JSON.stringify(config))
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
