import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  getOrganizationCreditAllocation,
  getOrganizationCreditInvoice,
  getOrganizationCreditRepayment,
  createOrganizationCreditTerm,
  listOrganizationCreditAllocations,
  listOrganizationCreditInvoices,
  listOrganizationCreditRepayments,
  listOrganizationCreditTerms,
  updateOrganizationCreditTermStatus,
} from '@/lib/api/generated/sdk.gen'
import type { CreditTermCreateRequest, CreditTermStatusRequest } from '@/lib/api/generated/types.gen'

const PAGE_SIZE = 50

export function useOrganizationCreditTerms(organizationId: number, enabled: boolean) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'credit-terms'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCreditTerms({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before: pageParam, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useOrganizationCreditInvoices(organizationId: number, enabled: boolean) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'credit-invoices'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCreditInvoices({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before: pageParam, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useOrganizationCreditRepayments(organizationId: number, enabled: boolean) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'credit-repayments'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCreditRepayments({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before: pageParam, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useOrganizationCreditAllocations(organizationId: number, enabled: boolean) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'credit-allocations'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCreditAllocations({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before: pageParam, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function organizationCreditErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

export function useCreateOrganizationCreditTerm(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: CreditTermCreateRequest) => {
      const { data } = await createOrganizationCreditTerm({
        client: apiClient,
        path: { organization_id: organizationId },
        body,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'credit-terms') }),
  })
}

export function useUpdateOrganizationCreditTermStatus(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ termId, body }: { termId: number; body: CreditTermStatusRequest }) => {
      const { data } = await updateOrganizationCreditTermStatus({
        client: apiClient,
        path: { organization_id: organizationId, term_id: termId },
        body,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'credit-terms') }),
  })
}

export async function getCreditInvoice(organizationId: number, invoiceId: number) {
  const { data } = await getOrganizationCreditInvoice({
    client: apiClient,
    path: { organization_id: organizationId, invoice_id: invoiceId },
  })
  return data
}

export async function getCreditRepayment(organizationId: number, repaymentId: number) {
  const { data } = await getOrganizationCreditRepayment({
    client: apiClient,
    path: { organization_id: organizationId, repayment_id: repaymentId },
  })
  return data
}

export async function getCreditAllocation(organizationId: number, allocationId: number) {
  const { data } = await getOrganizationCreditAllocation({
    client: apiClient,
    path: { organization_id: organizationId, allocation_id: allocationId },
  })
  return data
}
