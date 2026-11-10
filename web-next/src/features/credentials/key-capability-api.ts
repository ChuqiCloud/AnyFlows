import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient, ApiError } from '@/lib/api'

const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const
export const capabilityKey = (channelId: number, credentialId?: number) => ['upstream-key-capabilities', channelId, credentialId ?? 'channel'] as const
export const credentialUrl = (channelId: number, credentialId: number) => `/api/admin/channels/${channelId}/credentials/${credentialId}`
export type UpstreamPolicy = { mode: 'inherit' | 'allowlist' | 'denylist'; models: string[]; disabled_models: string[] }
export type ModelPolicyConfig = { policy: UpstreamPolicy; version: number; auto_add_discovered: boolean; refresh_interval_secs: number | null }
export type ModelObservation = ModelPolicyConfig & {
  items: Array<{ upstream_model: string; canonical_models: string[]; enabled: boolean; blocked_reason: string | null; source: string }>
  last_success_at: number | null; last_checked_at: number | null; status: string; error_code: string | null
}
export type UsageSnapshot = {
  status: 'available' | 'unsupported' | 'unavailable'; windows: Array<{ window_seconds: number; used_percent: number; reset_at: number | null }>
  credits_balance: string | null; fetched_at: number | null
  scope?: 'key' | 'account' | 'window'; provider?: string; currency?: string | null; unit?: string | null
  total?: string | null; used?: string | null; remaining?: string | null; unlimited?: boolean
  expires_at?: number | null; reset_at?: number | null; last_success_at?: number | null; last_checked_at?: number | null
  stale?: boolean; error_code?: string | null; account_ref?: string | null
  source?: string; pause_until?: number | null; alert?: string | null
  subscription?: { items: Array<{ name: string; expires_at: string | null; daily_limit_usd: string | null; daily_used_usd: string | null; monthly_limit_usd: string | null; monthly_used_usd: string | null }> } | null
}
export type UsageConfig = {
  version: number; provider: 'auto' | 'codex' | 'new_api' | 'one_api' | 'sub2api' | 'none'; scope: 'key' | 'account'
  profile_id: number | null; refresh_interval_secs: number | null; quota_per_unit: string | null
  currency: string | null; low_balance_threshold: string | null; pause_when_exhausted: boolean
  inherited?: boolean
}
export type UsageProfile = {
  id: number; name: string; provider: 'new_api' | 'one_api' | 'sub2api'; base_url: string; platform_user_id: string | null
  quota_per_unit: string | null; currency: string | null; version: number; has_secret: boolean
}
export type UsageProfileWrite = Omit<UsageProfile, 'id' | 'has_secret'> & { secret: string | null }

export async function capabilityGet<T>(url: string, signal?: AbortSignal) {
  return (await apiClient.get<{ 200: T }, unknown, true>({ url, security, signal, throwOnError: true })).data
}
export async function capabilityWrite<T>(url: string, body: unknown, method: 'put' | 'post' = 'put') {
  return (await apiClient[method]<{ 200: T; 201: T }, unknown, true>({ url, body, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true })).data
}
export function capabilityError(error: unknown) {
  return error instanceof ApiError && error.status === 409 ? 'conflict' : error instanceof ApiError && error.status === 403 ? 'forbidden' : error instanceof ApiError && error.status === 400 ? 'invalid' : 'error'
}
function policyUrl(channelId: number, credentialId?: number) {
  return `${credentialId === undefined ? `/api/admin/channels/${channelId}` : credentialUrl(channelId, credentialId)}/model-policy`
}
export function useUpstreamPolicy(channelId: number, credentialId?: number) {
  return useQuery({ queryKey: [...capabilityKey(channelId, credentialId), 'policy'], refetchOnWindowFocus: false, queryFn: ({ signal }) => capabilityGet<ModelPolicyConfig>(policyUrl(channelId, credentialId), signal) })
}
export function useWriteUpstreamPolicy(channelId: number, credentialId?: number) {
  const client = useQueryClient()
  return useMutation({ mutationFn: (body: ModelPolicyConfig) => capabilityWrite<ModelPolicyConfig>(policyUrl(channelId, credentialId), body), onSuccess: async (data) => {
    client.setQueryData([...capabilityKey(channelId, credentialId), 'policy'], data)
    await Promise.all([client.invalidateQueries({ queryKey: credentialId === undefined ? ['upstream-key-capabilities', channelId] : [...capabilityKey(channelId, credentialId), 'models'] }), client.invalidateQueries({ queryKey: ['upstream-key-capabilities', channelId, 'coverage'] }), client.invalidateQueries({ queryKey: ['model-catalog'] }), client.invalidateQueries({ queryKey: ['admin-channel', channelId] }), client.invalidateQueries({ queryKey: ['admin-channels'] })])
  } })
}
export function useCredentialModels(channelId: number, credentialId: number, enabled = true) {
  return useQuery({ queryKey: [...capabilityKey(channelId, credentialId), 'models'], enabled, refetchOnWindowFocus: false, queryFn: ({ signal }) => capabilityGet<ModelObservation>(`${credentialUrl(channelId, credentialId)}/models`, signal) })
}
export async function refreshCredentialModels(channelId: number, credentialId: number) {
  return capabilityWrite<ModelObservation>(`${credentialUrl(channelId, credentialId)}/models/refresh`, {}, 'post')
}
export function useRefreshCredentialModels(channelId: number, credentialId: number) {
  const client = useQueryClient()
  return useMutation({ mutationFn: () => refreshCredentialModels(channelId, credentialId), onSuccess: async (data) => {
    client.setQueryData([...capabilityKey(channelId, credentialId), 'models'], data)
    client.setQueryData([...capabilityKey(channelId, credentialId), 'policy'], data)
    await client.invalidateQueries({ queryKey: ['model-catalog'] })
  } })
}
export function useUpstreamUsage(channelId: number, credentialId: number) {
  return useQuery({ queryKey: [...capabilityKey(channelId, credentialId), 'usage'], refetchInterval: 60_000, refetchIntervalInBackground: false, queryFn: ({ signal }) => capabilityGet<UsageSnapshot>(`${credentialUrl(channelId, credentialId)}/usage`, signal) })
}
export function useRefreshUpstreamUsage(channelId: number, credentialId: number) {
  const client = useQueryClient()
  return useMutation({ mutationFn: () => capabilityWrite<UsageSnapshot>(`${credentialUrl(channelId, credentialId)}/usage/refresh`, {}, 'post'), onSuccess: (data) => client.setQueryData([...capabilityKey(channelId, credentialId), 'usage'], data) })
}
function usageConfigUrl(channelId: number, credentialId?: number) { return `${credentialId === undefined ? `/api/admin/channels/${channelId}` : credentialUrl(channelId, credentialId)}/usage-config` }
export function useUsageConfig(channelId: number, credentialId?: number) {
  return useQuery({ queryKey: [...capabilityKey(channelId, credentialId), 'usage-config'], refetchOnWindowFocus: false, queryFn: ({ signal }) => capabilityGet<UsageConfig>(usageConfigUrl(channelId, credentialId), signal) })
}
export function useWriteUsageConfig(channelId: number, credentialId?: number) {
  const client = useQueryClient()
  return useMutation({ mutationFn: (body: UsageConfig) => capabilityWrite<UsageConfig>(usageConfigUrl(channelId, credentialId), body), onSuccess: async (data) => {
    client.setQueryData([...capabilityKey(channelId, credentialId), 'usage-config'], data)
    await client.invalidateQueries({ queryKey: credentialId === undefined ? ['upstream-key-capabilities', channelId] : [...capabilityKey(channelId, credentialId), 'usage'] })
  } })
}
export function useResetUsageConfig(channelId: number, credentialId?: number) {
  const client = useQueryClient()
  return useMutation({ mutationFn: async () => { if (credentialId === undefined) throw new Error('A Key is required'); await apiClient.delete({ url: usageConfigUrl(channelId, credentialId), security, throwOnError: true }) }, onSuccess: () => client.invalidateQueries({ queryKey: capabilityKey(channelId, credentialId) }) })
}
export const usageProfilesKey = ['upstream-usage-profiles'] as const
export function useUsageProfiles() {
  return useQuery({ queryKey: usageProfilesKey, queryFn: ({ signal }) => capabilityGet<{ items: UsageProfile[] }>('/api/admin/upstream-usage-profiles', signal) })
}
export function useWriteUsageProfile() {
  const client = useQueryClient()
  return useMutation({ gcTime: 0, mutationFn: ({ id, body }: { id?: number; body: UsageProfileWrite }) => capabilityWrite<UsageProfile>(`/api/admin/upstream-usage-profiles${id === undefined ? '' : `/${id}`}`, body, id === undefined ? 'post' : 'put'), onSuccess: () => Promise.all([client.invalidateQueries({ queryKey: usageProfilesKey }), client.invalidateQueries({ predicate: (query) => query.queryKey[0] === 'upstream-key-capabilities' && query.queryKey.at(-1) === 'usage' })]) })
}
export function useDeleteUsageProfile() {
  const client = useQueryClient()
  return useMutation({ mutationFn: async (id: number) => { await apiClient.delete({ url: `/api/admin/upstream-usage-profiles/${id}`, security, throwOnError: true }) }, onSuccess: () => client.invalidateQueries({ queryKey: usageProfilesKey }) })
}
