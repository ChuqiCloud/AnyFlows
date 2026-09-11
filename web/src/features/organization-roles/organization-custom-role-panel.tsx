import { KeyRound, Pencil, Plus, RefreshCw, ShieldBan, ShieldCheck } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { OrganizationCustomRoleResponse } from '@/lib/api/generated/types.gen'
import { useOrganizationCustomRoles } from './organization-custom-role-api'
import { OrganizationCustomRoleDisableDialog } from './organization-custom-role-disable-dialog'
import { OrganizationCustomRoleEditorSheet } from './organization-custom-role-editor-sheet'
import { selectedCustomRolePermissions } from './organization-custom-role-permissions'

const ROLES_READ = 'organization.roles.read'
const ROLES_MANAGE = 'organization.roles.manage'

/** 在企业设置内管理角色目录，不创建额外路由或脱离当前企业上下文。 */
export function OrganizationCustomRolePanel({
  id,
  organizationId,
  organizationName,
  customRolesEnabled,
  permissions,
}: {
  id?: string
  organizationId: number
  organizationName: string
  customRolesEnabled: boolean
  permissions: readonly string[]
}) {
  const { t } = useTranslation()
  const canRead = permissions.includes(ROLES_READ)
  const canManage = permissions.includes(ROLES_MANAGE)
  const query = useOrganizationCustomRoles(organizationId, customRolesEnabled && canRead)
  const [editor, setEditor] = useState<OrganizationCustomRoleResponse | null | undefined>()
  const [disabling, setDisabling] = useState<OrganizationCustomRoleResponse>()
  const roles = useMemo(() => query.data?.pages.flatMap((page) => page.roles) ?? [], [query.data])

  const openCreate = () => setEditor(null)
  const openEdit = (role: OrganizationCustomRoleResponse) => setEditor(role)

  return (
    <Card id={id} className="scroll-mt-4 overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><KeyRound className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <CardTitle className="text-base">{t('organizationConsole.customRoles.title')}</CardTitle>
                {customRolesEnabled && canRead && query.isSuccess ? <Badge className="bg-surface-2 text-muted-foreground">{roles.length}</Badge> : null}
                {!customRolesEnabled || !canRead ? <Badge className="text-muted-foreground">{t('organizationConsole.common.restricted')}</Badge> : null}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.customRoles.subtitle')}</p>
            </div>
          </div>
          {customRolesEnabled && canRead ? (
            <div className="flex flex-wrap gap-2">
              <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={query.isFetching} onClick={() => void query.refetch()}>
                <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
                <span className="sr-only">{t('organizationConsole.actions.refresh')}</span>
              </Button>
              {canManage ? <Button type="button" size="sm" onClick={openCreate}><Plus aria-hidden="true" />{t('organizationConsole.customRoles.actions.create')}</Button> : null}
            </div>
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {!customRolesEnabled ? <RolePanelState icon={ShieldBan} title={t('organizationConsole.customRoles.restricted.entitlementTitle')} body={t('organizationConsole.customRoles.restricted.entitlementBody')} /> : null}
        {customRolesEnabled && !canRead ? <RolePanelState icon={ShieldBan} title={t('organizationConsole.customRoles.restricted.permissionTitle')} body={t('organizationConsole.customRoles.restricted.permissionBody')} /> : null}
        {customRolesEnabled && canRead && query.isPending ? <RolePanelLoading /> : null}
        {customRolesEnabled && canRead && query.isError && roles.length === 0 ? <RolePanelState icon={ShieldBan} title={t('organizationConsole.customRoles.errors.listTitle')} body={t('organizationConsole.customRoles.errors.listBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} /> : null}
        {customRolesEnabled && canRead && query.isSuccess && roles.length === 0 ? <RolePanelState icon={ShieldCheck} title={t('organizationConsole.customRoles.empty.title')} body={t(canManage ? 'organizationConsole.customRoles.empty.manageBody' : 'organizationConsole.customRoles.empty.readBody')} action={canManage ? <Button type="button" size="sm" onClick={openCreate}><Plus aria-hidden="true" />{t('organizationConsole.customRoles.actions.create')}</Button> : undefined} /> : null}
        {customRolesEnabled && canRead && roles.length > 0 ? (
          <>
            <div className="divide-y divide-[var(--hairline)]">
              {roles.map((role) => (
                <RoleRow key={role.role_key} role={role} canManage={canManage} onEdit={() => openEdit(role)} onDisable={() => setDisabling(role)} />
              ))}
            </div>
            {query.hasNextPage ? <div className="flex justify-center border-t border-[var(--hairline)] p-4"><Button type="button" size="sm" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t(query.isFetchingNextPage ? 'organizationConsole.actions.loadingMore' : 'organizationConsole.actions.loadMore')}</Button></div> : null}
          </>
        ) : null}
      </CardContent>
      <OrganizationCustomRoleEditorSheet organizationId={organizationId} organizationName={organizationName} role={editor ?? undefined} open={editor !== undefined} onOpenChange={(open) => { if (!open) setEditor(undefined) }} />
      <OrganizationCustomRoleDisableDialog organizationId={organizationId} role={disabling} onClose={() => setDisabling(undefined)} />
    </Card>
  )
}

function RoleRow({
  role,
  canManage,
  onEdit,
  onDisable,
}: {
  role: OrganizationCustomRoleResponse
  canManage: boolean
  onEdit: () => void
  onDisable: () => void
}) {
  const { t } = useTranslation()
  const active = role.status === 'active'
  const permissions = selectedCustomRolePermissions(role.permissions)
  return (
    <article className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="min-w-0 truncate text-sm font-medium" title={role.name}>{role.name}</h3>
          <Badge className={active ? 'text-success' : 'text-muted-foreground'}>{t(`organizationConsole.customRoles.status.${role.status}`)}</Badge>
        </div>
        {role.description ? <p className="mt-1 text-xs leading-5 text-muted-foreground">{role.description}</p> : <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.customRoles.noDescription')}</p>}
        <details className="mt-2">
          <summary className="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground">{t('organizationConsole.customRoles.list.permissionCount', { count: permissions.length })}</summary>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {permissions.map((permission) => <Badge key={permission} className="h-auto whitespace-normal py-1 text-muted-foreground">{t(`organizationConsole.permissions.${permission}`)}</Badge>)}
          </div>
        </details>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <span className="mr-1 text-xs text-muted-foreground tabular-nums">v{role.version}</span>
        {canManage && active ? (
          <>
            <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.edit')} onClick={onEdit}>
              <Pencil aria-hidden="true" />
              <span className="sr-only">{t('organizationConsole.actions.edit')}</span>
            </Button>
            <Button type="button" size="icon-sm" variant="destructive" title={t('organizationConsole.customRoles.actions.disable')} onClick={onDisable}>
              <ShieldBan aria-hidden="true" />
              <span className="sr-only">{t('organizationConsole.customRoles.actions.disable')}</span>
            </Button>
          </>
        ) : null}
      </div>
    </article>
  )
}

function RolePanelLoading() {
  return <div className="grid gap-3 p-4" role="status">{[0, 1, 2].map((item) => <div key={item} className="grid grid-cols-[minmax(0,1fr)_5rem] gap-3"><div className="grid gap-2"><Skeleton className="h-4 w-2/5" /><Skeleton className="h-3 w-4/5" /><Skeleton className="h-3 w-1/3" /></div><Skeleton className="h-8 w-20" /></div>)}</div>
}

function RolePanelState({ icon: Icon, title, body, action }: { icon: LucideIcon; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-52 place-items-center p-6 text-center"><div className="max-w-sm"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
