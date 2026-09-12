import { useQueryClient, type UseQueryResult } from '@tanstack/react-query'
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'

import {
  clearOrganizationWorkspaceCache,
  useOrganizationOverview,
  useOrganizationDirectory,
} from '@/features/organization-budgets/organization-budget-api'
import type { OrganizationSummaryResponse } from '@/lib/api/generated/types.gen'
import { getManagementSessionToken } from '@/lib/api/session-token'

const ORGANIZATION_WORKSPACE_KEY = 'anyflows.organization.workspace'

type OrganizationWorkspaceContextValue = {
  organizations: OrganizationSummaryResponse[]
  organizationsQuery: UseQueryResult<OrganizationSummaryResponse[], Error>
  selectedId?: number
  selectedOrganization?: OrganizationSummaryResponse
  /** 当前企业服务端权限投影；个人空间或尚未加载时为空。 */
  organizationPermissions?: string[]
  organizationPermissionsLoading: boolean
  switching: boolean
  selectOrganization: (organizationId: number) => Promise<void>
  selectPersonalWorkspace: () => Promise<void>
}

const OrganizationWorkspaceContext = createContext<OrganizationWorkspaceContextValue | undefined>(undefined)

type OrganizationWorkspaceProviderProps = {
  children: ReactNode
  enterpriseView: boolean
}

/** 统一维护企业工作空间偏好；本地偏好只决定前端入口，服务端仍逐请求复验成员关系。 */
export function OrganizationWorkspaceProvider({ children, enterpriseView }: OrganizationWorkspaceProviderProps) {
  const queryClient = useQueryClient()
  const organizationsQuery = useOrganizationDirectory(Boolean(getManagementSessionToken()))
  const organizations = useMemo(() => organizationsQuery.data ?? [], [organizationsQuery.data])
  const [preferredId, setPreferredId] = useState(readStoredOrganizationId)
  const [switching, setSwitching] = useState(false)
  const selectedId = enterpriseView ? preferredId : undefined
  const organizationOverviewQuery = useOrganizationOverview(selectedId ?? 0, selectedId !== undefined)

  useEffect(() => {
    if (!organizationsQuery.data) return
    const selectedAvailable = preferredId !== undefined
      && organizations.some((organization) => organization.id === preferredId)
    if (selectedAvailable) return

    if (preferredId !== undefined) {
      void clearOrganizationWorkspaceCache(queryClient, preferredId)
      storeOrganizationId(undefined)
    }
    setPreferredId(enterpriseView ? organizations[0]?.id : undefined)
    if (enterpriseView && organizations[0]) storeOrganizationId(organizations[0].id)
  }, [enterpriseView, organizations, organizationsQuery.data, preferredId, queryClient])

  const changeSelection = useCallback(async (nextId?: number) => {
    if (nextId === preferredId || switching) return
    setSwitching(true)
    try {
      if (preferredId !== undefined) await clearOrganizationWorkspaceCache(queryClient, preferredId)
      storeOrganizationId(nextId)
      setPreferredId(nextId)
    } finally {
      setSwitching(false)
    }
  }, [preferredId, queryClient, switching])

  const selectPersonalWorkspace = useCallback(async () => {
    if (preferredId === undefined || switching) return
    setSwitching(true)
    try {
      await clearOrganizationWorkspaceCache(queryClient, preferredId)
    } finally {
      setSwitching(false)
    }
  }, [preferredId, queryClient, switching])

  const value = useMemo<OrganizationWorkspaceContextValue>(() => ({
    organizations,
    organizationsQuery,
    selectedId,
    selectedOrganization: organizations.find((organization) => organization.id === selectedId),
    organizationPermissions: organizationOverviewQuery.data?.permissions,
    organizationPermissionsLoading: selectedId !== undefined && organizationOverviewQuery.isPending,
    switching,
    selectOrganization: changeSelection,
    selectPersonalWorkspace,
  }), [changeSelection, organizationOverviewQuery.data?.permissions, organizationOverviewQuery.isPending, organizations, organizationsQuery, selectPersonalWorkspace, selectedId, switching])

  return (
    <OrganizationWorkspaceContext.Provider value={value}>
      {children}
    </OrganizationWorkspaceContext.Provider>
  )
}

/** 读取当前控制台唯一企业工作空间上下文。 */
export function useOrganizationWorkspace() {
  const context = useContext(OrganizationWorkspaceContext)
  if (!context) throw new Error('企业工作空间上下文未挂载')
  return context
}

function readStoredOrganizationId() {
  try {
    const value = window.localStorage.getItem(ORGANIZATION_WORKSPACE_KEY)
    if (!value || !/^[1-9]\d*$/.test(value)) return undefined
    const parsed = Number(value)
    return Number.isSafeInteger(parsed) ? parsed : undefined
  } catch {
    return undefined
  }
}

function storeOrganizationId(organizationId?: number) {
  try {
    if (organizationId === undefined) window.localStorage.removeItem(ORGANIZATION_WORKSPACE_KEY)
    else window.localStorage.setItem(ORGANIZATION_WORKSPACE_KEY, String(organizationId))
  } catch {
    // 浏览器拒绝持久化时仅影响偏好恢复，不改变当前会话的服务端授权边界。
  }
}
