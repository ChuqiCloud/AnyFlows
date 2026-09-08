import { AlertTriangle, ArrowRightLeft, Ban, Building2, LoaderCircle, Plus, RefreshCw } from 'lucide-react'
import { useState, type FormEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import type { OrganizationDepartmentResponse, OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import {
  organizationDepartmentErrorStatus,
  useCreateOrganizationDepartment,
  useDisableOrganizationDepartment,
  useMoveOrganizationDepartment,
  useOrganizationDepartments,
} from './organization-department-api'

const DEPARTMENTS_READ = 'organization.departments.read'
const DEPARTMENTS_MANAGE = 'organization.departments.manage'

/** 企业部门目录管理页；所有写操作都以企业权限和部门版本为准。 */
export function OrganizationDepartmentPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={Building2} title={t('organizationDepartments.title')} subtitle={t('organizationDepartments.subtitle')} />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => <OrganizationDepartmentWorkspace key={organizationId} organizationId={organizationId} overview={overview} />}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationDepartmentWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const canRead = overview.permissions.includes(DEPARTMENTS_READ)
  const canManage = overview.permissions.includes(DEPARTMENTS_MANAGE)
  const query = useOrganizationDepartments(organizationId, canRead)
  const departments = query.data?.pages.flatMap((page) => page.departments) ?? []
  const activeDepartments = departments.filter((department) => department.status === 'active')
  const [createOpen, setCreateOpen] = useState(false)
  const [moving, setMoving] = useState<OrganizationDepartmentResponse>()
  const [disabling, setDisabling] = useState<OrganizationDepartmentResponse>()

  if (!canRead) {
    return <OrganizationPageState icon={Ban} title={t('organizationDepartments.restricted.title')} body={t('organizationDepartments.restricted.body')} />
  }

  const loadMore = () => {
    if (query.hasNextPage && !query.isFetchingNextPage) void query.fetchNextPage()
  }
  return (
    <>
      {canManage ? (
        <Card>
          <CardHeader className="flex-row items-start justify-between gap-3">
            <div>
              <CardTitle>{t('organizationDepartments.create.title')}</CardTitle>
              <CardDescription className="mt-1">{t('organizationDepartments.create.description')}</CardDescription>
            </div>
            <Button type="button" size="sm" variant={createOpen ? 'secondary' : 'default'} onClick={() => setCreateOpen((open) => !open)}>
              {createOpen ? <Ban aria-hidden="true" /> : <Plus aria-hidden="true" />}
              {createOpen ? t('organizationDepartments.actions.cancel') : t('organizationDepartments.actions.create')}
            </Button>
          </CardHeader>
          {createOpen ? <DepartmentCreateForm organizationId={organizationId} parents={activeDepartments} onCreated={() => setCreateOpen(false)} /> : null}
        </Card>
      ) : null}

      {moving ? <DepartmentMoveForm organizationId={organizationId} department={moving} parents={activeDepartments} onClose={() => setMoving(undefined)} /> : null}

      <Card className="overflow-hidden">
        <CardHeader className="flex-row items-start justify-between gap-3">
          <div>
            <CardTitle>{t('organizationDepartments.list.title')}</CardTitle>
            <CardDescription className="mt-1">{t('organizationDepartments.list.description')}</CardDescription>
          </div>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('organizationDepartments.actions.refresh')} onClick={() => void query.refetch()} disabled={query.isFetching}>
            <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          </Button>
        </CardHeader>
        <CardContent className="p-0">
          {query.isPending ? <DepartmentLoading /> : query.isError ? (
            <OrganizationPageState icon={AlertTriangle} title={t('organizationDepartments.errors.listTitle')} body={t('organizationDepartments.errors.listBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationDepartments.actions.retry')}</Button>} />
          ) : departments.length === 0 ? (
            <OrganizationPageState icon={Building2} title={t('organizationDepartments.empty.title')} body={t('organizationDepartments.empty.body')} />
          ) : (
            <>
              <div className="divide-y divide-[var(--hairline)]">
                {departments.map((department) => <DepartmentRow key={department.id} department={department} canManage={canManage} onMove={() => setMoving(department)} onDisable={() => setDisabling(department)} />)}
              </div>
              {query.hasNextPage ? <div className="flex justify-center border-t border-[var(--hairline)] p-3"><Button type="button" size="sm" variant="secondary" disabled={query.isFetchingNextPage} onClick={loadMore}>{query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{query.isFetchingNextPage ? t('organizationDepartments.actions.loading') : t('organizationDepartments.actions.loadMore')}</Button></div> : null}
            </>
          )}
        </CardContent>
      </Card>

      <DepartmentDisableDialog organizationId={organizationId} department={disabling} onClose={() => setDisabling(undefined)} />
    </>
  )
}

function DepartmentCreateForm({ organizationId, parents, onCreated }: { organizationId: number; parents: OrganizationDepartmentResponse[]; onCreated: () => void }) {
  const { t } = useTranslation()
  const mutation = useCreateOrganizationDepartment(organizationId)
  const [name, setName] = useState('')
  const [parentId, setParentId] = useState('')
  const [sortOrder, setSortOrder] = useState('0')
  const valid = name.trim().length > 0 && name === name.trim() && name.length <= 128 && /^-?\d+$/.test(sortOrder)
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || mutation.isPending) return
    try {
      await mutation.mutateAsync({ name, parent_department_id: parentId ? Number(parentId) : null, sort_order: Number(sortOrder) })
      setName(''); setParentId(''); setSortOrder('0'); mutation.reset(); onCreated()
    } catch {
      // 保留输入内容，便于处理 CAS、容量或服务暂时不可用。
    }
  }
  const error = mutation.isError ? (organizationDepartmentErrorStatus(mutation.error) === 409 ? t('organizationDepartments.errors.conflict') : t('organizationDepartments.errors.mutation')) : undefined
  return (
    <CardContent className="border-t border-[var(--hairline)] pt-4">
      <form className="grid gap-3 sm:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)_9rem_auto] sm:items-end" onSubmit={submit}>
        <label className="grid gap-1.5 text-xs font-medium"><span>{t('organizationDepartments.fields.name')}</span><Input value={name} maxLength={128} disabled={mutation.isPending} onChange={(event) => { setName(event.target.value); mutation.reset() }} /></label>
        <label className="grid gap-1.5 text-xs font-medium"><span>{t('organizationDepartments.fields.parent')}</span><Select value={parentId} disabled={mutation.isPending} onChange={(event) => setParentId(event.target.value)}><option value="">{t('organizationDepartments.fields.root')}</option>{parents.map((parent) => <option key={parent.id} value={parent.id}>{parent.name}</option>)}</Select></label>
        <label className="grid gap-1.5 text-xs font-medium"><span>{t('organizationDepartments.fields.sortOrder')}</span><Input inputMode="numeric" value={sortOrder} disabled={mutation.isPending} onChange={(event) => setSortOrder(event.target.value)} /></label>
        <Button type="submit" size="sm" disabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}{t('organizationDepartments.actions.create')}</Button>
      </form>
      {error ? <p role="alert" className="mt-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{error}</p> : null}
    </CardContent>
  )
}

function DepartmentMoveForm({ organizationId, department, parents, onClose }: { organizationId: number; department: OrganizationDepartmentResponse; parents: OrganizationDepartmentResponse[]; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useMoveOrganizationDepartment(organizationId)
  const [parentId, setParentId] = useState(department.parent_department_id?.toString() ?? '')
  const candidates = parents.filter((candidate) => candidate.id !== department.id && candidate.id !== department.parent_department_id)
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (mutation.isPending) return
    try {
      await mutation.mutateAsync({ departmentId: department.id, body: { parent_department_id: parentId ? Number(parentId) : null, expected_version: department.version } })
      onClose()
    } catch {
      // 冲突时保留当前版本和选择，不自动覆盖服务端新树。
    }
  }
  const error = mutation.isError ? (organizationDepartmentErrorStatus(mutation.error) === 409 ? t('organizationDepartments.errors.conflict') : t('organizationDepartments.errors.mutation')) : undefined
  return (
    <Card className="border-brand/35 bg-brand/5">
      <CardHeader className="pb-3"><CardTitle className="text-sm">{t('organizationDepartments.move.title', { name: department.name })}</CardTitle><CardDescription>{t('organizationDepartments.move.description')}</CardDescription></CardHeader>
      <CardContent>
        <form className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-end" onSubmit={submit}>
          <label className="grid gap-1.5 text-xs font-medium"><span>{t('organizationDepartments.fields.parent')}</span><Select value={parentId} disabled={mutation.isPending} onChange={(event) => setParentId(event.target.value)}><option value="">{t('organizationDepartments.fields.root')}</option>{candidates.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.name}</option>)}</Select></label>
          <Button type="button" size="sm" variant="secondary" onClick={onClose} disabled={mutation.isPending}>{t('organizationDepartments.actions.cancel')}</Button>
          <Button type="submit" size="sm" disabled={mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ArrowRightLeft aria-hidden="true" />}{t('organizationDepartments.actions.move')}</Button>
        </form>
        {error ? <p role="alert" className="mt-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{error}</p> : null}
      </CardContent>
    </Card>
  )
}

function DepartmentRow({ department, canManage, onMove, onDisable }: { department: OrganizationDepartmentResponse; canManage: boolean; onMove: () => void; onDisable: () => void }) {
  const { t } = useTranslation()
  const active = department.status === 'active'
  return (
    <article className="grid min-w-0 gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="min-w-0" style={{ paddingLeft: `${Math.min(department.depth, 8) * 0.75}rem` }}>
        <div className="flex min-w-0 flex-wrap items-center gap-2"><Building2 className="size-3.5 shrink-0 text-brand" aria-hidden="true" /><span className="truncate text-sm font-medium">{department.name}</span><Badge className={active ? 'text-success' : 'text-muted-foreground'}>{t(`organizationDepartments.status.${department.status}`)}</Badge></div>
        <p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{department.department_key} · {t('organizationDepartments.row.depth', { depth: department.depth })} · {t('organizationDepartments.row.version', { version: department.version })}</p>
      </div>
      {canManage && active ? <div className="flex items-center justify-end gap-1"><Button type="button" size="icon-sm" variant="ghost" aria-label={t('organizationDepartments.actions.move')} onClick={onMove}><ArrowRightLeft aria-hidden="true" /></Button><Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationDepartments.actions.disable')} onClick={onDisable}><Ban aria-hidden="true" /></Button></div> : null}
    </article>
  )
}

function DepartmentDisableDialog({ organizationId, department, onClose }: { organizationId: number; department?: OrganizationDepartmentResponse; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useDisableOrganizationDepartment(organizationId)
  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!department || mutation.isPending) return
    try { await mutation.mutateAsync({ departmentId: department.id, body: { expected_version: department.version } }); mutation.reset(); onClose() } catch { /* 保留目标，允许刷新后重试。 */ }
  }
  const error = mutation.isError ? (organizationDepartmentErrorStatus(mutation.error) === 409 ? t('organizationDepartments.errors.conflict') : t('organizationDepartments.errors.mutation')) : undefined
  return (
    <AlertDialog open={department !== undefined} onOpenChange={(open) => { if (!open && !mutation.isPending) { mutation.reset(); onClose() } }}>
      <AlertDialogContent>
        <AlertDialogHeader><AlertDialogTitle>{t('organizationDepartments.disable.title')}</AlertDialogTitle><AlertDialogDescription>{t('organizationDepartments.disable.description', { name: department?.name })}</AlertDialogDescription></AlertDialogHeader>
        {error ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{error}</p> : null}
        <AlertDialogFooter><AlertDialogCancel disabled={mutation.isPending}>{t('organizationDepartments.actions.cancel')}</AlertDialogCancel><AlertDialogAction disabled={mutation.isPending} onClick={confirm}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Ban aria-hidden="true" />}{t('organizationDepartments.actions.disable')}</AlertDialogAction></AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function DepartmentLoading() {
  return <div className="grid gap-2 p-4" role="status">{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-14 rounded-lg" />)}</div>
}
