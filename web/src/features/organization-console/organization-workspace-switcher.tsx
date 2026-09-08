import { Building2, CircleUserRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Select } from '@/components/ui/select'
import { cn } from '@/lib/utils'
import { useOrganizationWorkspace } from './organization-workspace-context'

const PERSONAL_WORKSPACE = 'personal'

/** 顶栏工作空间切换器；企业切换进入企业概览，个人切换回到个人控制台。 */
export function OrganizationWorkspaceSwitcher() {
  const { t } = useTranslation()
  const workspace = useOrganizationWorkspace()
  const value = workspace.selectedId === undefined
    ? PERSONAL_WORKSPACE
    : `organization:${workspace.selectedId}`

  if (workspace.organizationsQuery.isPending || workspace.organizations.length === 0) return null

  const changeWorkspace = async (nextValue: string) => {
    if (nextValue === PERSONAL_WORKSPACE) {
      await workspace.selectPersonalWorkspace()
      window.location.hash = '#/console'
      return
    }

    const organizationId = Number(nextValue.slice('organization:'.length))
    if (!Number.isSafeInteger(organizationId) || organizationId <= 0) return
    await workspace.selectOrganization(organizationId)
    if (!window.location.hash.startsWith('#/console/organization')) {
      window.location.hash = '#/console/organization'
    }
  }

  return (
    <div className="relative hidden min-w-0 sm:block">
      {workspace.selectedId === undefined
        ? <CircleUserRound className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
        : <Building2 className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-brand" aria-hidden="true" />}
      <Select
        aria-label={t('organizationConsole.workspace.label')}
        className="h-7 w-[clamp(8rem,18vw,13rem)] truncate pl-7 text-xs"
        value={value}
        disabled={workspace.switching}
        onChange={(event) => void changeWorkspace(event.target.value)}
      >
        <option value={PERSONAL_WORKSPACE}>{t('organizationConsole.workspace.personal')}</option>
        {workspace.organizations.map((organization) => (
          <option key={organization.id} value={`organization:${organization.id}`}>
            {organization.name}
          </option>
        ))}
      </Select>
    </div>
  )
}

/** 窄屏页面内保留企业选择器，避免顶栏空间不足时丢失当前企业上下文。 */
export function OrganizationMobileWorkspaceSelect({ className }: { className?: string }) {
  const { t } = useTranslation()
  const workspace = useOrganizationWorkspace()
  if (workspace.organizations.length === 0) return null

  return (
    <div className={cn('w-full min-w-0 sm:hidden', className)}>
      <label htmlFor="organization-mobile-workspace" className="mb-1.5 block text-[0.6875rem] font-medium text-muted-foreground">
        {t('organizationConsole.workspace.label')}
      </label>
      <Select
        id="organization-mobile-workspace"
        value={workspace.selectedId ?? ''}
        disabled={workspace.switching}
        onChange={(event) => void workspace.selectOrganization(Number(event.target.value))}
      >
        {workspace.organizations.map((organization) => (
          <option key={organization.id} value={organization.id}>
            {organization.name} ({organization.slug})
          </option>
        ))}
      </Select>
    </div>
  )
}
