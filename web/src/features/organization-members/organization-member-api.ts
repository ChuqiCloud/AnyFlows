import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import {
  organizationDirectoryQueryKey,
  organizationQueryKey,
} from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  changeOrganizationMember,
  changeOrganizationTeam,
  createOrganizationInvitation,
  createOrganizationInvitationBatch,
  createOrganizationTeam,
  disableOrganizationTeam,
  listOrganizationCustomRoles,
  listOrganizationInvitations,
  listOrganizationMembers,
  listOrganizationTeams,
  previewOrganizationInvitations,
  resendOrganizationInvitation,
  revokeOrganizationInvitation,
  setOrganizationMemberCustomRole,
  transferOrganizationOwnership,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationInvitationBatchRequest,
  OrganizationInvitationCreateRequest,
  OrganizationInvitationPreviewRequest,
  OrganizationMemberCustomRoleRequest,
  OrganizationMembershipChangeRequest,
  OrganizationOwnershipTransferRequest,
  OrganizationTeamChangeRequest,
  OrganizationTeamCreateRequest,
} from '@/lib/api/generated/types.gen'

export function useOrganizationMembers(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'members', 'pages'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationMembers({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useOrganizationTeams(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'teams', 'pages'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationTeams({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useOrganizationInvitations(organizationId: number, enabled = true) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'invitations'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationInvitations({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

export function useOrganizationCustomRoles(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'custom-roles', 'pages'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCustomRoles({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after: pageParam, limit: 100 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useChangeOrganizationMember(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ membershipId, body }: { membershipId: number; body: OrganizationMembershipChangeRequest }) => {
      const { data } = await changeOrganizationMember({
        body,
        client: apiClient,
        path: { membership_id: membershipId, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateMembers(queryClient, organizationId),
  })
}

export function useSetOrganizationMemberCustomRole(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ membershipId, body }: { membershipId: number; body: OrganizationMemberCustomRoleRequest }) => {
      const { data } = await setOrganizationMemberCustomRole({
        body,
        client: apiClient,
        path: { membership_id: membershipId, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateMembers(queryClient, organizationId),
  })
}

export function useTransferOrganizationOwnership(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationOwnershipTransferRequest) => {
      const { data } = await transferOrganizationOwnership({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => Promise.all([
      invalidateMembers(queryClient, organizationId),
      queryClient.invalidateQueries({ queryKey: organizationDirectoryQueryKey }),
    ]),
  })
}

export function useCreateOrganizationTeam(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationTeamCreateRequest) => {
      const { data } = await createOrganizationTeam({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateTeams(queryClient, organizationId),
  })
}

export function useChangeOrganizationTeam(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ teamId, body }: { teamId: number; body: OrganizationTeamChangeRequest }) => {
      const { data } = await changeOrganizationTeam({
        body,
        client: apiClient,
        path: { organization_id: organizationId, team_id: teamId },
      })
      return data
    },
    onSuccess: () => invalidateTeams(queryClient, organizationId),
  })
}

export function useDisableOrganizationTeam(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ teamId, expectedVersion }: { teamId: number; expectedVersion: number }) => {
      const { data } = await disableOrganizationTeam({
        body: { expected_version: expectedVersion },
        client: apiClient,
        path: { organization_id: organizationId, team_id: teamId },
      })
      return data
    },
    onSuccess: () => invalidateTeams(queryClient, organizationId),
  })
}

export function useCreateOrganizationInvitation(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationInvitationCreateRequest) => {
      const { data } = await createOrganizationInvitation({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateInvitations(queryClient, organizationId),
  })
}

export function usePreviewOrganizationInvitations(organizationId: number) {
  return useMutation({
    mutationFn: async (body: OrganizationInvitationPreviewRequest) => {
      const { data } = await previewOrganizationInvitations({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
  })
}

export function useCreateOrganizationInvitationBatch(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationInvitationBatchRequest) => {
      const { data } = await createOrganizationInvitationBatch({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateInvitations(queryClient, organizationId),
  })
}

export function useRevokeOrganizationInvitation(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (invitationKey: string) => {
      const { data } = await revokeOrganizationInvitation({
        client: apiClient,
        path: { invitation_key: invitationKey, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateInvitations(queryClient, organizationId),
  })
}

export function useResendOrganizationInvitation(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (invitationKey: string) => {
      const { data } = await resendOrganizationInvitation({
        client: apiClient,
        path: { invitation_key: invitationKey, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateInvitations(queryClient, organizationId),
  })
}

export function organizationMemberErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}

function invalidateMembers(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'members') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}

function invalidateTeams(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'teams') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'members') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}

function invalidateInvitations(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'invitations') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
