import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

export type VerificationKind = 'individual' | 'enterprise'
export type VerificationRecord = { id: number; user_id: number; kind: VerificationKind; subject_name: string; summary: string; status: number; version: number; review_reason?: string | null; reviewer_user_id?: number | null; created_at: number; updated_at: number }
export type VerificationMaterial = { id: number; kind: string; file_name: string; content_type: string; size_bytes: number; content_available?: boolean }
export type VerificationDetail = { case: VerificationRecord; materials: VerificationMaterial[] }
export const verificationKey = ['account-verification'] as const
export const sessionSecurity = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export async function verificationGet<T>(url: string, signal?: AbortSignal): Promise<T> {
  const result = await apiClient.get<T>({ url, signal, security: [...sessionSecurity] }) as unknown as { data: T }
  return result.data
}
export async function materialBlob(url: string, signal: AbortSignal): Promise<Blob> {
  const result = await apiClient.get({ url, signal, parseAs: 'blob', security: [...sessionSecurity] }) as unknown as { data: Blob }
  return result.data
}
export function verificationBase(admin: boolean) { return admin ? '/api/admin/account-verifications' : '/api/account/verifications' }

export function useAccountEligibility() {
  return useQuery({ queryKey: [...verificationKey, 'eligibility'], gcTime: 0, retry: false,
    refetchInterval: 30_000, refetchIntervalInBackground: false,
    queryFn: ({ signal }) => verificationGet<{ enterprise_verified: boolean; can_apply_for_organization: boolean }>('/api/account/verifications/eligibility', signal) })
}
export function useAccountVerifications(admin: boolean, status?: number) {
  return useInfiniteQuery({ queryKey: [...verificationKey, admin ? 'admin' : 'self', status], gcTime: 0, retry: false,
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam, signal }) => {
      const params = new URLSearchParams({ limit: '25' })
      if (pageParam !== undefined) params.set('before', String(pageParam))
      if (status !== undefined) params.set('status', String(status))
      return verificationGet<{ cases: VerificationRecord[]; next_cursor?: number | null }>(`${verificationBase(admin)}?${params}`, signal)
    }, getNextPageParam: (page) => page.next_cursor ?? undefined })
}
export function useVerificationDetail(id: number | undefined, admin: boolean) {
  return useQuery({ queryKey: [...verificationKey, 'detail', admin, id], enabled: id !== undefined, gcTime: 0, retry: false,
    queryFn: ({ signal }) => verificationGet<VerificationDetail>(`${verificationBase(admin)}/${id}`, signal) })
}
export function useSubmitAccountVerification() {
  const client = useQueryClient()
  return useMutation({ mutationFn: async ({ kind, subject_name, summary, files }: { kind: VerificationKind; subject_name: string; summary: string; files: File[] }) => {
    const data = new FormData()
    data.append('metadata', JSON.stringify({ kind, subject_name, summary, materials: files.map((_, index) => ({ kind: kind === 'enterprise' ? 'business_document' : 'identity_document', file_field: String(index) })) }))
    files.forEach((file, index) => data.append(`file:${index}`, file, file.name))
    const result = await apiClient.post<VerificationRecord>({ url: '/api/account/verifications', body: data, bodySerializer: null, headers: { 'Content-Type': null }, security: [...sessionSecurity] }) as unknown as { data: VerificationRecord }
    return result.data
  }, onSuccess: () => client.invalidateQueries({ queryKey: verificationKey }) })
}
export function useReviewAccountVerification() {
  const client = useQueryClient()
  return useMutation({ mutationFn: async ({ id, expected_version, status, reason }: { id: number; expected_version: number; status: number; reason?: string }) => {
    await apiClient.post({ url: `${verificationBase(true)}/${id}/decision`, body: { expected_version, status, reason }, security: [...sessionSecurity] })
  }, onSuccess: () => client.invalidateQueries({ queryKey: verificationKey }) })
}
