import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  decideOrganizationVerification,
  getOrganizationVerification,
  listAdminOrganizationVerifications,
  submitOrganizationVerification,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationVerificationDecisionRequest,
  OrganizationVerificationListResponse,
  OrganizationVerificationResponse,
  OrganizationVerificationStatus,
  OrganizationVerificationSubmitRequest,
} from '@/lib/api/generated/types.gen'

export type {
  OrganizationVerificationDecisionRequest,
  OrganizationVerificationListResponse,
  OrganizationVerificationResponse,
  OrganizationVerificationStatus,
  OrganizationVerificationSubmitRequest,
}

const verificationRoot = ['organization-verification'] as const

export function useOrganizationVerification(organizationId: number, enabled = true) {
  return useQuery({
    queryKey: [...verificationRoot, organizationId],
    enabled,
    retry: false,
    queryFn: async ({ signal }) => {
      try {
        const { data } = await getOrganizationVerification({ client: apiClient, path: { organization_id: organizationId }, signal })
        return data
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null
        throw error
      }
    },
  })
}

export function useSubmitOrganizationVerification(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationVerificationSubmitRequest) => {
      const { data } = await submitOrganizationVerification({ client: apiClient, path: { organization_id: organizationId }, body })
      return data
    },
    onSuccess: (data) => queryClient.setQueryData([...verificationRoot, organizationId], data),
  })
}

export function useAdminOrganizationVerifications(status?: OrganizationVerificationStatus) {
  return useInfiniteQuery({
    queryKey: [...verificationRoot, 'admin', status ?? 'all'],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminOrganizationVerifications({ client: apiClient, query: { status, before: pageParam, limit: 50 }, signal })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

export function useDecideOrganizationVerification() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ caseId, body }: { caseId: number; body: OrganizationVerificationDecisionRequest }) => {
      const { data } = await decideOrganizationVerification({ client: apiClient, path: { case_id: caseId }, body })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: [...verificationRoot, 'admin'] }),
  })
}

export function organizationVerificationErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

