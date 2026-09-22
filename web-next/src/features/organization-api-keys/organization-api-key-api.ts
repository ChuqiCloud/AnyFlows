import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationToken,
  disableOrganizationToken,
  disableOwnOrganizationToken,
  listOrganizationTokenDirectory,
  listOrganizationTokens,
  updateOrganizationToken,
} from '@/lib/api/generated/sdk.gen'
import type { OrganizationTokenWriteRequest } from '@/lib/api/generated/types.gen'

type OrganizationTokenScope = 'directory' | 'self'

function organizationTokenQueryRoot(organizationId: number) {
  return ['organization', organizationId, 'tokens'] as const
}

function organizationTokenQueryKey(
  organizationId: number,
  scope: OrganizationTokenScope,
  after: number | undefined,
  limit: number,
) {
  return [...organizationTokenQueryRoot(organizationId), scope, after ?? null, limit] as const
}

/** 按企业和当前成员双重范围读取本人 Key，游标不会跨工作空间复用。 */
export function useOrganizationApiKeys(
  organizationId: number,
  after: number | undefined,
  limit: number,
  enabled: boolean,
) {
  return useQuery({
    queryKey: organizationTokenQueryKey(organizationId, 'self', after, limit),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationTokens({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after, limit },
        signal,
      })
      return data
    },
  })
}

/** 仅在权限投影允许时读取企业脱敏目录。 */
export function useOrganizationApiKeyDirectory(
  organizationId: number,
  after: number | undefined,
  limit: number,
  enabled: boolean,
) {
  return useQuery({
    queryKey: organizationTokenQueryKey(organizationId, 'directory', after, limit),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationTokenDirectory({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after, limit },
        signal,
      })
      return data
    },
  })
}

export function useCreateOrganizationApiKey(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: OrganizationTokenWriteRequest) => {
      const { data } = await createOrganizationToken({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationTokens(queryClient, organizationId),
  })
}

export function useUpdateOrganizationApiKey(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: OrganizationTokenWriteRequest }) => {
      const { data } = await updateOrganizationToken({
        body,
        client: apiClient,
        path: { organization_id: organizationId, token_id: id },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationTokens(queryClient, organizationId),
  })
}

export function useDisableOrganizationApiKey(organizationId: number, directory: boolean) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      const request = directory ? disableOrganizationToken : disableOwnOrganizationToken
      const { data } = await request({
        client: apiClient,
        path: { organization_id: organizationId, token_id: id },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationTokens(queryClient, organizationId),
  })
}

/** 读取企业 Key 接口公开错误码，兼容生成契约尚未枚举的企业授权错误。 */
export function organizationApiKeyErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

export function organizationApiKeyErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

function invalidateOrganizationTokens(
  queryClient: ReturnType<typeof useQueryClient>,
  organizationId: number,
) {
  return queryClient.invalidateQueries({ queryKey: organizationTokenQueryRoot(organizationId) })
}
