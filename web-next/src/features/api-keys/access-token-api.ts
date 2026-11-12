import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'

export type AccessTokenScope = {
  scope: string
  label: string
  description: string
  admin_only: boolean
}

export type AccessTokenPreset = {
  id: string
  label: string
  scopes: string[]
}

export type AccessTokenCatalog = {
  scopes: AccessTokenScope[]
  presets: AccessTokenPreset[]
  default_expiry_days: number
  max_expiry_days: number
  allow_permanent: boolean
}

export type AccessToken = {
  id: number
  name: string
  token_hint: string
  scopes: string[]
  enabled: boolean
  expires_at: number | null
  created_at: number
  updated_at: number
  last_used_at: number | null
  last_used_ip: string | null
  allow_ips: string[] | null
  version: number
}

export type AccessTokenPage = {
  tokens: AccessToken[]
  next_cursor: number | null
  capacity: number
}

export type AccessTokenWrite = {
  name: string
  scopes: string[]
  expires_at: number | null
  allow_ips: string[] | null
  proof?: string
  enabled?: boolean
  version?: number
}

export type IssuedAccessToken = { token: AccessToken; secret: string }

export type AccessTokenPolicy = {
  enabled: boolean
  issuance_enabled: boolean
  max_tokens_per_user: number
  version: number
}

export type AccessTokenIdentity = { userId: number; role: string }

export const accessTokensQueryKey = ['user-access-tokens'] as const
export const accessTokenCatalogQueryKey = ['user-access-token-catalog'] as const
export const accessTokenPolicyQueryKey = ['admin-access-token-policy'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

function identityKey(identity: AccessTokenIdentity | undefined) {
  return [identity?.userId ?? null, identity?.role ?? null] as const
}

async function get<T>(url: string, signal?: AbortSignal, query?: Record<string, unknown>): Promise<T> {
  return (await apiClient.get<{ 200: T }, unknown, true>({ url, security, signal, query, throwOnError: true })).data
}

export function useAccessTokens(identity: AccessTokenIdentity | undefined, limit = 50) {
  return useInfiniteQuery({
    queryKey: [...accessTokensQueryKey, ...identityKey(identity), limit],
    enabled: identity !== undefined,
    initialPageParam: undefined as number | undefined,
    queryFn: ({ signal, pageParam }) => identity
      ? get<AccessTokenPage>('/api/account/access-tokens', signal, { after: pageParam, limit })
      : Promise.reject(new Error('An authenticated account is required')),
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  })
}

export function useAccessTokenCatalog(identity: AccessTokenIdentity | undefined) {
  return useQuery({
    queryKey: [...accessTokenCatalogQueryKey, ...identityKey(identity)],
    enabled: identity !== undefined,
    queryFn: ({ signal }) => identity
      ? get<AccessTokenCatalog>('/api/account/access-tokens/scopes', signal)
      : Promise.reject(new Error('An authenticated account is required')),
    staleTime: 60_000,
  })
}

export function useAccessTokenPolicy(identity: AccessTokenIdentity | undefined) {
  return useQuery({
    queryKey: [...accessTokenPolicyQueryKey, ...identityKey(identity)],
    queryFn: ({ signal }) => identity?.role === 'admin'
      ? get<AccessTokenPolicy>('/api/admin/access-tokens/policy', signal)
      : Promise.reject(new Error('An administrator session is required')),
    enabled: identity?.role === 'admin',
  })
}

export function useCreateAccessToken() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: AccessTokenWrite) => (await apiClient.post<
      { 201: IssuedAccessToken; 200: IssuedAccessToken }, unknown, true
    >({ url: '/api/account/access-tokens', body, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true })).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: accessTokensQueryKey }),
  })
}

export function useUpdateAccessToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AccessTokenWrite }) => (await apiClient.patch<
      { 200: AccessToken }, unknown, true
    >({ url: `/api/account/access-tokens/${id}`, body, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true })).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: accessTokensQueryKey }),
  })
}

export function useRevokeAccessToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => apiClient.delete({ url: `/api/account/access-tokens/${id}`, security, throwOnError: true }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: accessTokensQueryKey }),
  })
}

export function useRevokeOwnAccessTokens() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async () => (await apiClient.post<{ 200: { revoked: number } }, unknown, true>({
      url: '/api/account/access-tokens/revoke-all', body: {}, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true,
    })).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: accessTokensQueryKey }),
  })
}

export function useUpdateAccessTokenPolicy(identity: AccessTokenIdentity | undefined) {
  const queryClient = useQueryClient()
  return useMutation({
    onMutate: () => identity ? [...accessTokenPolicyQueryKey, ...identityKey(identity)] : undefined,
    mutationFn: async ({ body, version }: { body: Omit<AccessTokenPolicy, 'version'>; version: number }) => {
      if (identity?.role !== 'admin') throw new Error('An administrator session is required')
      return (await apiClient.put<{ 200: AccessTokenPolicy }, unknown, true>({
        url: '/api/admin/access-tokens/policy', body: { ...body, version }, security,
        headers: { 'Content-Type': 'application/json' }, throwOnError: true,
      })).data
    },
    onSuccess: (policy, _variables, cacheKey) => {
      if (cacheKey) queryClient.setQueryData(cacheKey, policy)
    },
  })
}

export function useRevokeAllUserAccessTokens() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (userId: number) => (await apiClient.post<{ 200: { revoked: number } }, unknown, true>({
      url: `/api/admin/users/${userId}/access-tokens/revoke-all`, body: {}, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true,
    })).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: accessTokensQueryKey }),
  })
}

export type AccessTokenProofRequest = {
  action: 'create' | 'update'
  token_id?: number
  payload: Omit<AccessTokenWrite, 'proof'>
}

export async function requestAccessTokenProof(body: AccessTokenProofRequest) {
  return (await apiClient.post<{ 200: { proof: string; expires_at: number } }, unknown, true>({
    url: '/api/account/access-tokens/verify', body, security, headers: { 'Content-Type': 'application/json' }, throwOnError: true,
  })).data
}
