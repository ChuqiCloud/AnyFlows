import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'

const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const
const headers = { 'Content-Type': 'application/json' }

export type ExternalConnector = {
  id: string
  displayName: string
  authenticationModes: string[]
}

export type ExternalBalance = {
  amount: string | null
  unit: string | null
  status: 'available' | 'unavailable' | 'authentication_required'
  observedAt: number
}

export type ExternalAccount = {
  id: number
  connectorId: string
  externalSubject: string
  displayName: string | null
  baseUrl: string
  createdAt: number
  updatedAt: number
  latestBalance: ExternalBalance | null
}

export type ExternalAccountLink = {
  connector_id: string
  base_url: string
  credential: string
}

export type ExternalAccountSnapshotReviewer = {
  pluginId: string
  displayName: string
  pluginVersion: string
  supportsExternalReads: boolean
}

export type ExternalAccountSnapshotReviewCheck = {
  code: string
  title: string
  status: 'pass' | 'info' | 'warning' | 'fail'
  message: string
  recommendation: string | null
}

export type ExternalAccountSnapshotReview = {
  pluginId: string
  displayName: string
  pluginVersion: string
  snapshotCount: number
  firstObservedAt: number
  lastObservedAt: number
  summary: string
  checks: ExternalAccountSnapshotReviewCheck[]
  uiViews: unknown[]
}

const externalAccountKey = ['user-external-accounts'] as const
const connectorKey = ['user-external-connectors'] as const

async function get<T>(url: string, signal?: AbortSignal, query?: Record<string, unknown>) {
  return (await apiClient.get<{ 200: T }, unknown, true>({ url, security, signal, query, throwOnError: true })).data
}

export function useExternalConnectors() {
  return useQuery({
    queryKey: connectorKey,
    queryFn: ({ signal }) => get<ExternalConnector[]>('/api/account/external-connectors', signal),
    staleTime: 60_000,
  })
}

export function useExternalAccounts() {
  return useQuery({
    queryKey: externalAccountKey,
    queryFn: ({ signal }) => get<{ items: ExternalAccount[] }>('/api/account/external-accounts', signal),
  })
}

export function useLinkExternalAccount() {
  const client = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: ExternalAccountLink) => (await apiClient.post<
      { 200: ExternalAccount }, unknown, true
    >({ url: '/api/account/external-accounts', body, security, headers, throwOnError: true })).data,
    onSuccess: () => client.invalidateQueries({ queryKey: externalAccountKey }),
  })
}

export function useRefreshExternalAccount() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => (await apiClient.post<
      { 200: ExternalAccount }, unknown, true
    >({ url: `/api/account/external-accounts/${id}/refresh`, security, throwOnError: true })).data,
    onSuccess: () => client.invalidateQueries({ queryKey: externalAccountKey }),
  })
}

export function useUnlinkExternalAccount() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete({ url: `/api/account/external-accounts/${id}`, security, throwOnError: true })
    },
    onSuccess: () => client.invalidateQueries({ queryKey: externalAccountKey }),
  })
}

export function useExternalBalanceHistory(id: number | undefined, enabled: boolean) {
  return useQuery({
    queryKey: [...externalAccountKey, id, 'balances'],
    enabled: id !== undefined && enabled,
    queryFn: ({ signal }) => get<{ connectionId: number; items: ExternalBalance[] }>(
      `/api/account/external-accounts/${id}/balances`, signal, { limit: 10 },
    ),
  })
}

export function useExternalAccountSnapshotReviewers(enabled = true) {
  return useQuery({
    queryKey: [...connectorKey, 'snapshot-reviewers'],
    enabled,
    queryFn: ({ signal }) => get<{ plugins: ExternalAccountSnapshotReviewer[] }>(
      '/api/account/external-accounts/plugin-reviewers', signal,
    ),
    staleTime: 30_000,
    retry: false,
  })
}

export function useReviewExternalAccountSnapshots() {
  return useMutation({
    mutationFn: async ({ connectionId, pluginId, allowExternalReads }: { connectionId: number; pluginId: string; allowExternalReads: boolean }) => (await apiClient.post<
      { 200: ExternalAccountSnapshotReview }, unknown, true
    >({
      url: `/api/account/external-accounts/${connectionId}/plugin-review`,
      body: { plugin_id: pluginId, allow_external_reads: allowExternalReads },
      security,
      headers,
      throwOnError: true,
    })).data,
    gcTime: 0,
  })
}
