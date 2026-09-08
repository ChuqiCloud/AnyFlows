import { Ban, Bot, CheckCircle2, KeyRound, LoaderCircle, Plus, RefreshCw, RotateCw, ShieldCheck, ShieldX, WalletCards } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Skeleton } from '@/components/ui/skeleton'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  OrganizationPageBoundary,
  OrganizationPageHeader,
  OrganizationPageState,
  useOrganizationPageState,
} from '@/features/organization-console/organization-page-shell'
import { useOrganizationBudgets, useOrganizationTeamCatalog } from '@/features/organization-budgets/organization-budget-api'
import type { OrganizationServiceAccountIssuedResponse, OrganizationServiceAccountResponse } from '@/lib/api/generated/types.gen'
import {
  organizationServiceAccountErrorStatus,
  useChangeOrganizationServiceAccountStatus,
  useOrganizationServiceAccounts,
  useRotateOrganizationServiceAccountKey,
} from './organization-service-account-api'
import { OrganizationServiceAccountBudgetSheet } from './organization-service-account-budget-sheet'
import { OrganizationServiceAccountEditorSheet } from './organization-service-account-editor-sheet'
import { OrganizationServiceAccountKeyDialog } from './organization-service-account-key-dialog'

const SERVICE_ACCOUNT_READ = 'organization.service_accounts.read'
const SERVICE_ACCOUNT_MANAGE = 'organization.service_accounts.manage'
const BUDGET_MANAGE = 'organization.budgets.manage'
const PAGE_SIZE = 20

export function OrganizationServiceAccountPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader
        icon={Bot}
        title={t('organizationServiceAccounts.title')}
        subtitle={t('organizationServiceAccounts.subtitle')}
      />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => (
          <OrganizationServiceAccountWorkspace key={organizationId} organizationId={organizationId} permissions={overview.permissions} />
        )}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationServiceAccountWorkspace({
  organizationId,
  permissions,
}: {
  organizationId: number
  permissions: string[]
}) {
  const { i18n, t } = useTranslation()
  const canRead = permissions.includes(SERVICE_ACCOUNT_READ)
  const canManage = permissions.includes(SERVICE_ACCOUNT_MANAGE)
  const canManageBudget = canManage && permissions.includes(BUDGET_MANAGE)
  const [cursors, setCursors] = useState<Array<string | undefined>>([undefined])
  const [creating, setCreating] = useState(false)
  const [issued, setIssued] = useState<OrganizationServiceAccountIssuedResponse>()
  const [budgetAccount, setBudgetAccount] = useState<OrganizationServiceAccountResponse>()
  const [statusTarget, setStatusTarget] = useState<{ account: OrganizationServiceAccountResponse; next: string }>()
  const [rotateTarget, setRotateTarget] = useState<OrganizationServiceAccountResponse>()
  const after = cursors[cursors.length - 1]
  const accountsQuery = useOrganizationServiceAccounts(organizationId, after, PAGE_SIZE, canRead)
  const budgetsQuery = useOrganizationBudgets(organizationId, canManageBudget)
  const teamsQuery = useOrganizationTeamCatalog(organizationId, canManageBudget)
  const statusMutation = useChangeOrganizationServiceAccountStatus(organizationId)
  const rotateMutation = useRotateOrganizationServiceAccountKey(organizationId)
  const resetStatusMutation = statusMutation.reset
  const resetRotateMutation = rotateMutation.reset
  const dateFormat = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])

  useEffect(() => {
    resetStatusMutation()
    resetRotateMutation()
  }, [resetRotateMutation, resetStatusMutation, statusTarget, rotateTarget])

  if (!canRead) {
    return <OrganizationPageState icon={ShieldX} title={t('organizationServiceAccounts.errors.permissionTitle')} body={t('organizationServiceAccounts.errors.permissionBody')} />
  }

  const accounts = accountsQuery.data?.service_accounts ?? []
  const nextCursor = accountsQuery.data?.next_cursor ?? undefined
  const refreshing = accountsQuery.isFetching || budgetsQuery.isFetching
  const queryError = organizationServiceAccountErrorStatus(accountsQuery.error)
  const budgets = budgetsQuery.data?.policies ?? []
  const teams = teamsQuery.data ?? []

  const refresh = async () => {
    await Promise.all([accountsQuery.refetch(), ...(canManageBudget ? [budgetsQuery.refetch(), teamsQuery.refetch()] : [])])
  }

  const changeStatus = async () => {
    if (!statusTarget) return
    try {
      await statusMutation.mutateAsync({
        serviceAccountKey: statusTarget.account.service_account_key,
        body: { status: statusTarget.next, expected_version: statusTarget.account.version },
      })
      setStatusTarget(undefined)
    } catch {
      // 保留对话框，冲突或结果未知时要求管理员先刷新事实。
    }
  }

  const rotateKey = async () => {
    if (!rotateTarget) return
    try {
      const nextIssued = await rotateMutation.mutateAsync({
        serviceAccountKey: rotateTarget.service_account_key,
        body: { expected_version: rotateTarget.version },
      })
      rotateMutation.reset()
      setRotateTarget(undefined)
      setIssued(nextIssued)
    } catch {
      // 旋转失败时保留确认对话框，不展示任何密钥材料。
    }
  }

  return (
    <div className="grid min-w-0 gap-4">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 border-y border-[var(--hairline)] py-3">
        <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <ShieldCheck className="size-3.5 shrink-0 text-brand" aria-hidden="true" />
          <span>{t('organizationServiceAccounts.summary.page', { count: accounts.length, page: cursors.length })}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={refreshing} onClick={() => void refresh()}>
            <RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('organizationServiceAccounts.actions.refresh')}
          </Button>
          <Button type="button" size="sm" disabled={!canManage || accountsQuery.isPending} onClick={() => setCreating(true)}>
            <Plus aria-hidden="true" />
            {t('organizationServiceAccounts.actions.create')}
          </Button>
        </div>
      </div>

      {!canManage ? <p className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning"><ShieldX className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('organizationServiceAccounts.readOnly')}</p> : null}

      {accountsQuery.isPending ? (
        <ServiceAccountLoading />
      ) : accountsQuery.isError ? (
        <OrganizationPageState
          icon={queryError === 403 ? ShieldX : undefined}
          title={t(queryError === 403 ? 'organizationServiceAccounts.errors.permissionTitle' : 'organizationServiceAccounts.errors.listTitle')}
          body={t(queryError === 403 ? 'organizationServiceAccounts.errors.permissionBody' : 'organizationServiceAccounts.errors.listBody')}
          action={queryError === 403 ? undefined : <Button type="button" size="sm" variant="secondary" onClick={() => void accountsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationServiceAccounts.actions.retry')}</Button>}
        />
      ) : (
        <ServiceAccountList
          accounts={accounts}
          canManage={canManage}
          dateFormat={dateFormat}
          onBudget={setBudgetAccount}
          onRotate={setRotateTarget}
          onStatus={(account, next) => setStatusTarget({ account, next })}
        />
      )}

      {!accountsQuery.isPending && !accountsQuery.isError ? (
        <div className="flex items-center justify-between gap-2 border-t border-[var(--hairline)] pt-3">
          <Button type="button" size="sm" variant="secondary" disabled={cursors.length <= 1 || accountsQuery.isFetching} onClick={() => setCursors((current) => current.slice(0, -1))}>{t('organizationServiceAccounts.actions.previous')}</Button>
          <span className="text-xs text-muted-foreground">{t('organizationServiceAccounts.summary.pageNumber', { page: cursors.length })}</span>
          <Button type="button" size="sm" variant="secondary" disabled={!nextCursor || accountsQuery.isFetching} onClick={() => nextCursor && setCursors((current) => [...current, nextCursor])}>{t('organizationServiceAccounts.actions.next')}</Button>
        </div>
      ) : null}

      <OrganizationServiceAccountEditorSheet organizationId={organizationId} open={creating} onOpenChange={setCreating} onIssued={setIssued} />
      <OrganizationServiceAccountKeyDialog issued={issued} onClose={() => setIssued(undefined)} />
      <OrganizationServiceAccountBudgetSheet
        organizationId={organizationId}
        account={budgetAccount}
        policies={budgets}
        teams={teams}
        canManage={canManageBudget}
        open={budgetAccount !== undefined}
        onOpenChange={(open) => !open && setBudgetAccount(undefined)}
      />
      <ServiceAccountStatusDialog target={statusTarget} mutation={statusMutation} onClose={() => setStatusTarget(undefined)} onConfirm={() => void changeStatus()} />
      <ServiceAccountRotateDialog target={rotateTarget} mutation={rotateMutation} onClose={() => setRotateTarget(undefined)} onConfirm={() => void rotateKey()} />
    </div>
  )
}

function ServiceAccountList({
  accounts,
  canManage,
  dateFormat,
  onBudget,
  onRotate,
  onStatus,
}: {
  accounts: OrganizationServiceAccountResponse[]
  canManage: boolean
  dateFormat: Intl.DateTimeFormat
  onBudget: (account: OrganizationServiceAccountResponse) => void
  onRotate: (account: OrganizationServiceAccountResponse) => void
  onStatus: (account: OrganizationServiceAccountResponse, next: string) => void
}) {
  const { t } = useTranslation()
  if (accounts.length === 0) {
    return <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center"><div className="max-w-xs"><div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Bot className="size-4" aria-hidden="true" /></div><h3 className="mt-3 text-sm font-semibold">{t('organizationServiceAccounts.empty.title')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationServiceAccounts.empty.body')}</p></div></div>
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 lg:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground"><tr><th className="w-[27%] px-3 py-2 font-medium">{t('organizationServiceAccounts.columns.account')}</th><th className="w-[14%] px-3 py-2 font-medium">{t('organizationServiceAccounts.columns.status')}</th><th className="w-[19%] px-3 py-2 font-medium">{t('organizationServiceAccounts.columns.created')}</th><th className="w-[19%] px-3 py-2 font-medium">{t('organizationServiceAccounts.columns.updated')}</th><th className="w-[21%] px-3 py-2"><span className="sr-only">{t('organizationServiceAccounts.columns.actions')}</span></th></tr></thead>
          <tbody className="divide-y divide-[var(--hairline)]">{accounts.map((account) => <tr key={account.service_account_key} className="hover:bg-surface-2/35"><td className="px-3 py-2.5"><AccountIdentity account={account} /></td><td className="px-3 py-2.5"><StatusBadge status={account.status} /></td><td className="px-3 py-2.5 text-muted-foreground">{dateFormat.format(account.created_at * 1_000)}</td><td className="px-3 py-2.5 text-muted-foreground">{dateFormat.format(account.updated_at * 1_000)}</td><td className="px-2 py-2.5"><AccountActions account={account} canManage={canManage} onBudget={onBudget} onRotate={onRotate} onStatus={onStatus} /></td></tr>)}</tbody>
        </table>
      </div>
      <div className="grid gap-2 lg:hidden">{accounts.map((account) => <article key={account.service_account_key} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3"><div className="flex items-start justify-between gap-3"><AccountIdentity account={account} /><AccountActions account={account} canManage={canManage} onBudget={onBudget} onRotate={onRotate} onStatus={onStatus} /></div><dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs"><div><dt className="text-muted-foreground">{t('organizationServiceAccounts.columns.created')}</dt><dd className="mt-1 text-muted-foreground">{dateFormat.format(account.created_at * 1_000)}</dd></div><div><dt className="text-muted-foreground">{t('organizationServiceAccounts.columns.updated')}</dt><dd className="mt-1 text-muted-foreground">{dateFormat.format(account.updated_at * 1_000)}</dd></div><div className="col-span-2"><dt className="text-muted-foreground">{t('organizationServiceAccounts.columns.version')}</dt><dd className="mt-1 font-mono">v{account.version}</dd></div></dl></article>)}</div>
    </>
  )
}

function AccountIdentity({ account }: { account: OrganizationServiceAccountResponse }) {
  return <div className="min-w-0"><div className="flex min-w-0 items-center gap-2"><Bot className="size-3.5 shrink-0 text-brand" aria-hidden="true" /><span className="truncate font-medium">{account.name}</span></div><div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{account.service_account_key}</div></div>
}

function StatusBadge({ status }: { status: string }) {
  const { t } = useTranslation()
  return <Badge className={status === 'active' ? 'bg-success/10 text-success' : status === 'suspended' ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'}>{t(`organizationServiceAccounts.status.${status}`, { defaultValue: status })}</Badge>
}

function AccountActions({
  account,
  canManage,
  onBudget,
  onRotate,
  onStatus,
}: {
  account: OrganizationServiceAccountResponse
  canManage: boolean
  onBudget: (account: OrganizationServiceAccountResponse) => void
  onRotate: (account: OrganizationServiceAccountResponse) => void
  onStatus: (account: OrganizationServiceAccountResponse, next: string) => void
}) {
  const { t } = useTranslation()
  if (!canManage) return null
  const nextStatus = account.status === 'active' ? 'suspended' : account.status === 'suspended' ? 'active' : undefined
  return <div className="flex items-center justify-end gap-1">
    <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className="size-9 md:size-8" aria-label={t('organizationServiceAccounts.actions.budget')} onClick={() => onBudget(account)}><WalletCards aria-hidden="true" /></Button></TooltipTrigger><TooltipContent>{t('organizationServiceAccounts.actions.budget')}</TooltipContent></Tooltip>
    {account.status !== 'revoked' ? <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className="size-9 md:size-8" aria-label={t('organizationServiceAccounts.actions.rotate')} onClick={() => onRotate(account)}><RotateCw aria-hidden="true" /></Button></TooltipTrigger><TooltipContent>{t('organizationServiceAccounts.actions.rotate')}</TooltipContent></Tooltip> : null}
    {nextStatus ? <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className="size-9 md:size-8" aria-label={t(`organizationServiceAccounts.actions.${nextStatus}`)} onClick={() => onStatus(account, nextStatus)}><PowerIcon status={account.status} /></Button></TooltipTrigger><TooltipContent>{t(`organizationServiceAccounts.actions.${nextStatus}`)}</TooltipContent></Tooltip> : null}
    {account.status !== 'revoked' ? <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className="size-9 text-muted-foreground hover:text-destructive md:size-8" aria-label={t('organizationServiceAccounts.actions.revoke')} onClick={() => onStatus(account, 'revoked')}><Ban aria-hidden="true" /></Button></TooltipTrigger><TooltipContent>{t('organizationServiceAccounts.actions.revoke')}</TooltipContent></Tooltip> : null}
  </div>
}

function PowerIcon({ status }: { status: string }) {
  return status === 'active' ? <ShieldX aria-hidden="true" /> : <CheckCircle2 aria-hidden="true" />
}

function ServiceAccountStatusDialog({ target, mutation, onClose, onConfirm }: { target?: { account: OrganizationServiceAccountResponse; next: string }; mutation: ReturnType<typeof useChangeOrganizationServiceAccountStatus>; onClose: () => void; onConfirm: () => void }) {
  const { t } = useTranslation()
  const conflict = organizationServiceAccountErrorStatus(mutation.error) === 409
  return <AlertDialog open={target !== undefined} onOpenChange={(open) => !open && onClose()}><AlertDialogContent><AlertDialogHeader><div className="grid size-10 place-items-center rounded-lg bg-warning/10 text-warning"><ShieldX className="size-4" aria-hidden="true" /></div><AlertDialogTitle>{t('organizationServiceAccounts.statusDialog.title')}</AlertDialogTitle><AlertDialogDescription>{t(`organizationServiceAccounts.statusDialog.description.${target?.next ?? 'suspended'}`, { name: target?.account.name })}</AlertDialogDescription></AlertDialogHeader>{mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(conflict ? 'organizationServiceAccounts.errors.conflict' : 'organizationServiceAccounts.errors.status')}</p> : null}<AlertDialogFooter><AlertDialogCancel disabled={mutation.isPending}>{t('organizationServiceAccounts.actions.cancel')}</AlertDialogCancel><AlertDialogAction disabled={mutation.isPending} onClick={onConfirm}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t(`organizationServiceAccounts.actions.${target?.next === 'revoked' ? 'revoke' : 'confirmStatus'}`)}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
}

function ServiceAccountRotateDialog({ target, mutation, onClose, onConfirm }: { target?: OrganizationServiceAccountResponse; mutation: ReturnType<typeof useRotateOrganizationServiceAccountKey>; onClose: () => void; onConfirm: () => void }) {
  const { t } = useTranslation()
  const conflict = organizationServiceAccountErrorStatus(mutation.error) === 409
  return <AlertDialog open={target !== undefined} onOpenChange={(open) => !open && onClose()}><AlertDialogContent><AlertDialogHeader><div className="grid size-10 place-items-center rounded-lg bg-warning/10 text-warning"><KeyRound className="size-4" aria-hidden="true" /></div><AlertDialogTitle>{t('organizationServiceAccounts.rotateDialog.title')}</AlertDialogTitle><AlertDialogDescription>{t('organizationServiceAccounts.rotateDialog.description', { name: target?.name })}</AlertDialogDescription></AlertDialogHeader>{mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(conflict ? 'organizationServiceAccounts.errors.conflict' : 'organizationServiceAccounts.errors.rotate')}</p> : null}<AlertDialogFooter><AlertDialogCancel disabled={mutation.isPending}>{t('organizationServiceAccounts.actions.cancel')}</AlertDialogCancel><AlertDialogAction disabled={mutation.isPending} onClick={onConfirm}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <RotateCw aria-hidden="true" />}{t('organizationServiceAccounts.actions.rotate')}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
}

function ServiceAccountLoading() {
  return <div className="grid gap-2" role="status">{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 w-full rounded-lg" />)}</div>
}
