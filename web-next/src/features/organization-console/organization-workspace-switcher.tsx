import { Building2, CircleUserRound } from 'lucide-react'
import { Select, SelectItem } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import { navigateTo } from '@/lib/router-navigation'
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
      navigateTo('/console')
      return
    }

    const organizationId = Number(nextValue.slice('organization:'.length))
    if (!Number.isSafeInteger(organizationId) || organizationId <= 0) return
    await workspace.selectOrganization(organizationId)
    if (!window.location.pathname.startsWith('/console/organization')) {
      navigateTo('/console/organization')
    }
  }

  const items = [
    {
      key: PERSONAL_WORKSPACE,
      label: t('organizationConsole.workspace.personal'),
    },
    ...workspace.organizations.map((organization) => ({
      key: `organization:${organization.id}`,
      label: organization.name,
    })),
  ]

  return (
    <Select
      aria-label={t('organizationConsole.workspace.label')}
      className="hidden min-w-0 sm:block"
      classNames={{
        base: 'w-[clamp(8rem,18vw,13rem)]',
        trigger: 'h-8 min-h-8 rounded-full border-0 bg-default-100 px-3 shadow-none',
        value: 'truncate text-xs text-default-600',
      }}
      isDisabled={workspace.switching}
      selectedKeys={new Set([value])}
      size="sm"
      startContent={
        workspace.selectedId === undefined
          ? <CircleUserRound className="size-3.5 shrink-0 text-default-500" aria-hidden="true" />
          : <Building2 className="size-3.5 shrink-0 text-primary" aria-hidden="true" />
      }
      variant="flat"
      items={items}
      onSelectionChange={(keys) => {
        const nextValue = Array.from(keys)[0]
        if (typeof nextValue === 'string') void changeWorkspace(nextValue)
      }}
    >
      {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
    </Select>
  )
}

/** 窄屏页面内保留企业选择器，避免顶栏空间不足时丢失当前企业上下文。 */
export function OrganizationMobileWorkspaceSelect({ className }: { className?: string }) {
  const { t } = useTranslation()
  const workspace = useOrganizationWorkspace()
  if (workspace.organizations.length === 0) return null

  const items = workspace.organizations.map((organization) => ({
    key: String(organization.id),
    label: `${organization.name} (${organization.slug})`,
  }))

  return (
    <div className={`w-full min-w-0 sm:hidden${className ? ` ${className}` : ''}`}>
      <label htmlFor="organization-mobile-workspace" className="mb-1.5 block text-tiny font-medium text-default-500">
        {t('organizationConsole.workspace.label')}
      </label>
      <Select
        aria-label={t('organizationConsole.workspace.label')}
        id="organization-mobile-workspace"
        classNames={{
          trigger: 'h-10 min-h-10 rounded-xl',
          value: 'truncate text-small',
        }}
        isDisabled={workspace.switching}
        selectedKeys={workspace.selectedId === undefined ? new Set<string>() : new Set([String(workspace.selectedId)])}
        onSelectionChange={(keys) => {
          const nextValue = Array.from(keys)[0]
          if (typeof nextValue === 'string') void workspace.selectOrganization(Number(nextValue))
        }}
        items={items}
      >
        {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
      </Select>
    </div>
  )
}
