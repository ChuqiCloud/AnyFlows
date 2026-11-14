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
  PluginDeploymentRequestWritable,
} from '@/lib/api/generated/types.gen'

const root = ['admin-plugin-lifecycle'] as const
const artifactsKey = [...root, 'artifacts'] as const
const deploymentsKey = [...root, 'deployments'] as const

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
