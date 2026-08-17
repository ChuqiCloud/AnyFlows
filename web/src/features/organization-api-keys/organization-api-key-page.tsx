import {
  Building2,
  KeyRound,
  Plus,
  RefreshCw,
  ShieldX,
  UserRound,
  UsersRound,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useCursorPagination } from '@/components/data-table/use-table-pagination'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { IssuedApiKeyDialog } from '@/features/api-keys/issued-api-key-dialog'
import { useOrganizationOverview } from '@/features/organization-budgets/organization-budget-api'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import { OrganizationMobileWorkspaceSelect } from '@/features/organization-console/organization-workspace-switcher'
import type {
  IssuedOrganizationTokenResponse,
  OrganizationOverviewResponse,
  OrganizationTokenResponse,
} from '@/lib/api/generated/types.gen'
import {
  organizationApiKeyErrorStatus,
  useOrganizationApiKeyDirectory,
  useOrganizationApiKeys,
} from './organization-api-key-api'
import { OrganizationApiKeyDisableDialog } from './organization-api-key-disable-dialog'
import { OrganizationApiKeyEditorSheet } from './organization-api-key-editor-sheet'
import {
  isOrganizationApiKeyAvailable,
  OrganizationApiKeyTable,
} from './organization-api-key-table'

const MANAGE_SELF_PERMISSION = 'organization.keys.manage_self'
const READ_ALL_PERMISSION = 'organization.keys.read_all'

type EditorState = OrganizationTokenResponse | 'create' | undefined
type WorkspaceMode = 'directory' | 'self'

/** 企业 Key 入口；企业切换会清理旧查询、游标、表单和一次性密钥状态。 */
export function OrganizationApiKeyPage() {
  const { t } = useTranslation()
  const workspace = useOrganizationWorkspace()

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex min-w-0 flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Building2 className="size-3.5" aria-hidden="true" />
            {t('organizationApiKeys.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('organizationApiKeys.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('organizationApiKeys.subtitle')}</p>
        </div>

        <OrganizationMobileWorkspaceSelect />
      </header>

      {workspace.organizationsQuery.isPending ? (
        <OrganizationDirectoryLoading />
      ) : workspace.organizationsQuery.isError && workspace.organizationsQuery.data === undefined ? (
        <PageState
          title={t('organizationApiKeys.errors.directoryTitle')}
          body={t('organizationApiKeys.errors.directoryBody')}
          action={<Button type="button" size="sm" variant="secondary" onClick={() => void workspace.organizationsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationApiKeys.actions.retry')}</Button>}
        />
      ) : workspace.organizations.length === 0 ? (
        <PageState icon={Building2} title={t('organizationApiKeys.workspace.emptyTitle')} body={t('organizationApiKeys.workspace.emptyBody')} />
      ) : workspace.selectedId === undefined || workspace.switching ? (
        <OrganizationWorkspaceLoading />
      ) : (
        <OrganizationApiKeyWorkspace key={workspace.selectedId} organizationId={workspace.selectedId} />
      )}
    </div>
  )
}

function OrganizationApiKeyWorkspace({ organizationId }: { organizationId: number }) {
  const { t } = useTranslation()
  const overviewQuery = useOrganizationOverview(organizationId)
  const selfPagination = useCursorPagination<number>()
  const directoryPagination = useCursorPagination<number>()
  const [mode, setMode] = useState<WorkspaceMode>('self')
  const [editor, setEditor] = useState<EditorState>()
  const [disabling, setDisabling] = useState<OrganizationTokenResponse>()
  const [issued, setIssued] = useState<IssuedOrganizationTokenResponse>()
  const permissions = overviewQuery.data?.permissions ?? []
  const canManageSelf = permissions.includes(MANAGE_SELF_PERMISSION)
  const canReadAll = permissions.includes(READ_ALL_PERMISSION)
  const selfQuery = useOrganizationApiKeys(
    organizationId,
    selfPagination.cursor,
    selfPagination.pageSize,
    canManageSelf && mode === 'self',
  )
  const directoryQuery = useOrganizationApiKeyDirectory(
    organizationId,
    directoryPagination.cursor,
    directoryPagination.pageSize,
    canReadAll && mode === 'directory',
  )

  useEffect(() => {
    if (!overviewQuery.data) return
    if (mode === 'self' && !canManageSelf && canReadAll) setMode('directory')
    if (mode === 'directory' && !canReadAll && canManageSelf) setMode('self')
  }, [canManageSelf, canReadAll, mode, overviewQuery.data])

  if (overviewQuery.isPending) return <OrganizationWorkspaceLoading />
  if (overviewQuery.isError || !overviewQuery.data) {
    const forbidden = organizationApiKeyErrorStatus(overviewQuery.error) === 403
    return (
      <PageState
        icon={forbidden ? ShieldX : undefined}
        title={t(forbidden ? 'organizationApiKeys.errors.forbiddenTitle' : 'organizationApiKeys.errors.overviewTitle')}
        body={t(forbidden ? 'organizationApiKeys.errors.forbiddenBody' : 'organizationApiKeys.errors.overviewBody')}
        action={forbidden ? undefined : <Button type="button" size="sm" variant="secondary" onClick={() => void overviewQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationApiKeys.actions.retry')}</Button>}
      />
    )
  }

  const overview = overviewQuery.data
  if (!canManageSelf && !canReadAll) {
    return (
      <>
        <OrganizationKeySummary overview={overview} />
        <PageState icon={ShieldX} title={t('organizationApiKeys.permission.title')} body={t('organizationApiKeys.permission.body')} />
      </>
    )
  }

  const activePagination = mode === 'self' ? selfPagination : directoryPagination
  const activeQuery = mode === 'self' ? selfQuery : directoryQuery
  const tokens = activeQuery.data?.tokens ?? []
  const nextCursor = activeQuery.data?.next_cursor
  const activeCount = tokens.filter(isOrganizationApiKeyAvailable).length
  const createAllowed = overview.entitlement.status === 'active'
  const queryForbidden = activeQuery.isError && organizationApiKeyErrorStatus(activeQuery.error) === 403
  const refresh = async () => {
    await Promise.all([overviewQuery.refetch(), activeQuery.refetch()])
  }

  return (
    <div className="grid min-w-0 gap-4">
      <OrganizationKeySummary overview={overview} />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        {canManageSelf && canReadAll ? (
          <div className="inline-flex w-fit items-center rounded-lg border border-[var(--hairline)] bg-surface-1 p-1" aria-label={t('organizationApiKeys.views.label')}>
            <Button type="button" size="sm" variant={mode === 'self' ? 'secondary' : 'ghost'} aria-pressed={mode === 'self'} onClick={() => setMode('self')}><UserRound aria-hidden="true" />{t('organizationApiKeys.views.self')}</Button>
            <Button type="button" size="sm" variant={mode === 'directory' ? 'secondary' : 'ghost'} aria-pressed={mode === 'directory'} onClick={() => setMode('directory')}><UsersRound aria-hidden="true" />{t('organizationApiKeys.views.directory')}</Button>
          </div>
        ) : (
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            {mode === 'self' ? <UserRound className="size-3.5" aria-hidden="true" /> : <UsersRound className="size-3.5" aria-hidden="true" />}
            {t(mode === 'self' ? 'organizationApiKeys.views.self' : 'organizationApiKeys.views.directory')}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={activeQuery.isFetching} onClick={() => void refresh()}>
            <RefreshCw className={activeQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('organizationApiKeys.actions.refresh')}
          </Button>
          {mode === 'self' ? (
            <Button type="button" size="sm" disabled={!createAllowed || activeQuery.isPending} onClick={() => setEditor('create')}>
              <Plus aria-hidden="true" />{t('organizationApiKeys.actions.create')}
            </Button>
          ) : null}
        </div>
      </div>

      <div className="flex min-h-8 flex-wrap items-center justify-between gap-2 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('organizationApiKeys.summary.page', { count: tokens.length, active: activeCount, page: activePagination.currentPage })}</span>
        <Badge className="bg-surface-2 text-muted-foreground">{t('organizationApiKeys.summary.capacity', { limit: overview.entitlement.organization_key_limit })}</Badge>
      </div>

      {!createAllowed && mode === 'self' ? (
        <div className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">
          <ShieldX className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          <span>{t('organizationApiKeys.entitlement.readOnly')}</span>
        </div>
      ) : null}

      {activeQuery.isPending ? (
        <TokenListLoading />
      ) : activeQuery.isError ? (
        <PageState
          icon={queryForbidden ? ShieldX : undefined}
          title={t(queryForbidden ? 'organizationApiKeys.errors.forbiddenTitle' : 'organizationApiKeys.errors.tokensTitle')}
          body={t(queryForbidden ? 'organizationApiKeys.errors.forbiddenBody' : 'organizationApiKeys.errors.tokensBody')}
          action={queryForbidden ? undefined : <Button type="button" size="sm" variant="secondary" onClick={() => void activeQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationApiKeys.actions.retry')}</Button>}
        />
      ) : (
        <OrganizationApiKeyTable mode={mode} tokens={tokens} onEdit={setEditor} onDisable={setDisabling} />
      )}

      {!activeQuery.isPending && !activeQuery.isError ? (
        <DataTablePagination
          currentPage={activePagination.currentPage}
          availablePageCount={activePagination.availablePageCount}
          pageSize={activePagination.pageSize}
          itemCount={tokens.length}
          hasNextPage={nextCursor != null || activePagination.hasLoadedNextPage}
          fetching={activeQuery.isFetching}
          onFirstPage={activePagination.goToFirstPage}
          onPreviousPage={activePagination.goToPreviousPage}
          onPageSelect={activePagination.selectPage}
          onNextPage={() => activePagination.goToNextPage(nextCursor)}
          onPageSizeChange={activePagination.setPageSize}
        />
      ) : null}

      <OrganizationApiKeyEditorSheet
        organizationId={organizationId}
        organizationName={overview.name}
        open={editor !== undefined}
        token={editor === 'create' ? undefined : editor}
        onOpenChange={(open) => !open && setEditor(undefined)}
        onIssued={setIssued}
      />
      <IssuedApiKeyDialog issued={issued} showPlayground={false} onClose={() => setIssued(undefined)} />
      <OrganizationApiKeyDisableDialog
        key={`${mode}-${disabling?.id ?? 'none'}`}
        organizationId={organizationId}
        organizationName={overview.name}
        directory={mode === 'directory'}
        token={disabling}
        onClose={() => setDisabling(undefined)}
      />
    </div>
  )
}

function OrganizationKeySummary({ overview }: { overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  return (
    <section className="grid gap-3 border-y border-[var(--hairline)] py-3 sm:grid-cols-[minmax(0,1.5fr)_repeat(2,minmax(0,1fr))] sm:items-center">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold">{overview.name}</h3>
          <Badge>{t(`organizationBudgets.roles.${overview.role}`, { defaultValue: overview.role })}</Badge>
          <Badge className="text-success">{t(`organizationBudgets.entitlement.${overview.entitlement.status}`, { defaultValue: overview.entitlement.status })}</Badge>
        </div>
        <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{overview.slug}</p>
      </div>
      <SummaryValue label={t('organizationApiKeys.summary.membership')} value={`#${overview.membership_id}`} />
      <SummaryValue label={t('organizationApiKeys.summary.keyLimit')} value={String(overview.entitlement.organization_key_limit)} />
    </section>
  )
}

function SummaryValue({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 sm:text-right">
      <p className="text-[0.6875rem] text-muted-foreground">{label}</p>
      <p className="mt-1 truncate text-xs font-semibold tabular-nums" title={value}>{value}</p>
    </div>
  )
}

function OrganizationDirectoryLoading() {
  return <div className="grid gap-3" role="status"><Skeleton className="h-10 w-full rounded-lg" /><OrganizationWorkspaceLoading /></div>
}

function OrganizationWorkspaceLoading() {
  return <div className="grid gap-4" role="status"><Skeleton className="h-20 w-full rounded-lg" /><TokenListLoading /></div>
}

function TokenListLoading() {
  return <div className="grid gap-2" role="status">{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 w-full rounded-lg" />)}</div>
}

function PageState({
  icon: Icon = KeyRound,
  title,
  body,
  action,
}: {
  icon?: LucideIcon
  title: string
  body: string
  action?: ReactNode
}) {
  return (
    <div className="grid min-h-64 place-items-center rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center">
      <div className="max-w-md">
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h3 className="mt-3 text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
