import type { MouseEvent } from 'react'
import { useState } from 'react'
import { KeyRound, LoaderCircle, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent,
  AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button, buttonVariants } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import {
  batchDeleteAdminCredentialIds,
  useAdminCredentialHealth,
  useAdminChannelCredentialCatalog,
  useDeleteAdminCredential,
  useUpdateAdminCredential,
} from './credential-api'
import { updateCredentialStatusRequest } from './credential-form-model'
import { CredentialItem } from './credential-item'
import { credentialRuntimeState, supportsSparkShadow } from './credential-model'

type CredentialListProps = {
  channel: AdminChannel
  credentials: readonly AdminCredential[]
  loading: boolean
  loadError: boolean
  refreshing: boolean
  refreshError: boolean
  loadingMore: boolean
  hasNextPage: boolean
  active: boolean
  onRefresh: () => Promise<unknown>
  onLoadMore: () => Promise<unknown>
  onEdit: (credential: AdminCredential) => void
}

/** 管理账号池的快捷启停、分页和安全删除，完整编辑交给类型化 Sheet。 */
export function CredentialList(props: CredentialListProps) {
  const { i18n, t } = useTranslation()
  const updateMutation = useUpdateAdminCredential()
  const deleteMutation = useDeleteAdminCredential()
  const [deleting, setDeleting] = useState<AdminCredential>()
  const [filter, setFilter] = useState('all')
  const [selected, setSelected] = useState<number[]>([])
  const [batch, setBatch] = useState<AdminCredential[]>([])
  const [batchPending, setBatchPending] = useState(false)
  const [batchError, setBatchError] = useState(false)
  const sparkChannel = supportsSparkShadow(props.channel)
  const deleteCatalogQuery = useAdminChannelCredentialCatalog(
    props.channel.id,
    props.active,
  )
  const deleteCatalogReady = !sparkChannel || deleteCatalogQuery.isSuccess
  const deleteCatalog = sparkChannel ? deleteCatalogQuery.data ?? [] : props.credentials
  const shadowCount = deleteCatalog.filter((item) => item.parent_id === deleting?.id).length
  const catalog = deleteCatalogQuery.data ?? props.credentials
  const healthQuery = useAdminCredentialHealth(props.channel.id, catalog.map((item) => item.id), props.active && deleteCatalogQuery.isSuccess)
  const healthById = new Map(healthQuery.data?.map((item) => [item.credential_id, item]))
  const healthFilter = filter === 'degraded' || filter === 'healthCooling'
  const visible = catalog.filter((item) => filter === 'all'
    || (filter === 'degraded' ? (healthById.get(item.id)?.weight_multiplier_micros ?? 1_000_000) < 1_000_000
      : filter === 'healthCooling' ? healthById.get(item.id)?.cooling
        : credentialRuntimeState(item) === filter))
  const selectedItems = catalog.filter((item) => selected.includes(item.id))
  const bulkReady = deleteCatalogQuery.isSuccess && !deleteCatalogQuery.isFetching && (!healthFilter || (healthQuery.isSuccess && healthQuery.data !== null && !healthQuery.isFetching))
  const toggleSelected = (id: number) => setSelected((ids) => ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id])
  const removeBatch = async () => {
    if (!batch.length || batch.length > 2048) return
    setBatchPending(true); setBatchError(false)
    try {
      const result = await batchDeleteAdminCredentialIds(props.channel.id, batch.map((item) => item.id))
      setSelected((ids) => ids.filter((id) => !result.deleted.includes(id)))
      setBatch((items) => items.filter((item) => result.failed.includes(item.id)))
      setBatchError(result.failed.length > 0)
      await Promise.all([props.onRefresh(), deleteCatalogQuery.refetch()])
    } catch { setBatchError(true) } finally { setBatchPending(false) }
  }


  const toggle = async (credential: AdminCredential) => {
    const status = credential.status === 'enabled' ? 'disabled' : 'enabled'
    const body = updateCredentialStatusRequest(credential, status)
    if (!body) return
    try {
      await updateMutation.mutateAsync({
        channelId: props.channel.id,
        credentialId: credential.id,
        body,
      })
    } catch {
      // 行内只呈现固定错误，服务端诊断与敏感字段留在安全边界内。
    }
  }

  const remove = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!deleting) return
    try {
      await deleteMutation.mutateAsync({ channelId: props.channel.id, credentialId: deleting.id })
      setDeleting(undefined)
    } catch {
      // 保留确认框和目标 ID，管理员可以原事实重试。
    }
  }

  return (
    <>
      <section aria-labelledby="credential-list-title">
        <div className="flex min-h-10 items-center justify-between gap-3 border-b border-[var(--hairline)] pb-2">
          <div><h2 id="credential-list-title" className="text-sm font-semibold">{t('credentials.list.title')}</h2><p className="mt-0.5 text-[0.6875rem] text-muted-foreground">{t('credentials.list.loaded', { count: props.credentials.length })}</p></div>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('credentials.actions.refresh')} disabled={props.refreshing} onClick={() => props.onRefresh()}><RefreshCw className={props.refreshing ? 'animate-spin' : undefined} aria-hidden="true" /></Button>
        </div>


        <div className="flex flex-wrap items-center gap-3 py-3 text-xs">
          <label className="flex items-center gap-2"><input type="checkbox" disabled={!bulkReady || visible.length === 0} checked={visible.length > 0 && visible.every((item) => selected.includes(item.id))} onChange={(event) => setSelected(event.target.checked ? visible.map((item) => item.id) : [])} />{t('credentials.batch.selectFiltered', { defaultValue: '全选筛选结果' })}</label>
          <select aria-label={t('credentials.batch.status', { defaultValue: '按状态筛选' })} value={filter} onChange={(event) => { setFilter(event.target.value); setSelected([]) }} className="rounded-md border border-[var(--hairline)] bg-transparent p-2">
            <option value="all">{t('credentials.batch.all', { defaultValue: '全部状态' })}</option>
            <option value="degraded" disabled={!healthQuery.data}>{t('credentials.health.degraded')}</option>
            <option value="healthCooling" disabled={!healthQuery.data}>{t('credentials.health.cooling')}</option>
            {['available', 'pending', 'authorizationPending', 'autoDisabled', 'cooling', 'disabled', 'paused'].map((value) => <option key={value} value={value}>{t(`credentials.runtime.${value}`)}</option>)}
          </select>
          <Button type="button" size="sm" variant="secondary" disabled={!bulkReady || !selectedItems.length || selectedItems.length > 2048} onClick={() => { setBatchError(false); setBatch(selectedItems) }}><Trash2 className="size-4" aria-hidden="true" />{t('credentials.batch.deleteSelected', { defaultValue: '删除所选（{{count}}）', count: selectedItems.length })}</Button>
          {deleteCatalogQuery.isFetching ? <LoaderCircle className="size-4 animate-spin" aria-label={t('credentials.list.loading')} /> : null}
          {deleteCatalogQuery.isError ? <span role="alert" className="text-destructive">{t('credentials.delete.catalogError')}</span> : null}
        </div>

        {props.loading ? (
          <div className="grid gap-2 py-3" aria-label={t('credentials.list.loading')}>{[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-xl" />)}</div>
        ) : props.loadError ? (
          <div role="alert" className="my-3 rounded-xl border border-destructive/25 bg-destructive/8 p-4"><p className="text-xs text-destructive">{t('credentials.errors.load')}</p><Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => props.onRefresh()}>{t('credentials.actions.retry')}</Button></div>
        ) : visible.length === 0 ? (
          <div className="grid min-h-48 place-items-center border-b border-[var(--hairline)] text-center"><div><KeyRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{t('credentials.empty.title')}</h3><p className="mt-1 max-w-sm text-xs leading-5 text-muted-foreground">{t('credentials.empty.description')}</p></div></div>
        ) : (
          <div className="divide-y divide-[var(--hairline)] border-b border-[var(--hairline)]">
            {visible.map((credential) => (
              <CredentialItem
                key={credential.id}
                channelType={props.channel.type}
                credential={credential}
                health={healthById.get(credential.id)}
                parent={credential.parent_id ? catalog.find((item) => item.id === credential.parent_id) : undefined}
                language={i18n.language}
                pending={updateMutation.isPending && updateMutation.variables?.credentialId === credential.id}
                active={props.active}
                selected={selected.includes(credential.id)}
                onSelect={() => toggleSelected(credential.id)}
                onToggle={(item) => { void toggle(item) }}
                onEdit={props.onEdit}
                onDelete={setDeleting}
                onRefresh={props.onRefresh}
              />
            ))}
          </div>
        )}

        {props.refreshError || updateMutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t(props.refreshError ? 'credentials.errors.refresh' : 'credentials.errors.update')}</p> : null}
        {props.hasNextPage ? <Button type="button" variant="secondary" className="mt-3 w-full" disabled={props.loadingMore} onClick={() => props.onLoadMore()}>{props.loadingMore ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('credentials.actions.loadMore')}</Button> : null}
      </section>

      <AlertDialog open={deleting !== undefined} onOpenChange={(open) => { if (!open && !deleteMutation.isPending) setDeleting(undefined) }}>
        <AlertDialogContent>
          <AlertDialogHeader><AlertDialogTitle>{t('credentials.delete.title')}</AlertDialogTitle><AlertDialogDescription>{t(!deleteCatalogReady ? deleteCatalogQuery.isError ? 'credentials.delete.catalogError' : 'credentials.delete.catalogLoading' : shadowCount > 0 ? 'credentials.delete.cascadeDescription' : 'credentials.delete.description', { id: deleting?.id ?? '', count: shadowCount })}</AlertDialogDescription></AlertDialogHeader>
          {deleteCatalogQuery.isError ? <Button type="button" size="sm" variant="secondary" onClick={() => deleteCatalogQuery.refetch()}>{t('credentials.actions.retry')}</Button> : null}
          {deleteMutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('credentials.errors.delete')}</p> : null}
          <AlertDialogFooter><AlertDialogCancel disabled={deleteMutation.isPending}>{t('credentials.actions.cancel')}</AlertDialogCancel><AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={deleteMutation.isPending || !deleteCatalogReady} onClick={remove}>{deleteMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}{t('credentials.actions.delete')}</AlertDialogAction></AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={batch.length > 0} onOpenChange={(open) => { if (!open && !batchPending) setBatch([]) }}>
        <AlertDialogContent>
          <AlertDialogHeader><AlertDialogTitle>{t('credentials.batch.confirmTitle', { defaultValue: '批量删除凭据' })}</AlertDialogTitle><AlertDialogDescription>{t('credentials.batch.confirm', { defaultValue: '将软删除 {{count}} 个凭据及其影子后代，相关路由绑定将失效。请确认所选状态和数量。', count: batch.length })}</AlertDialogDescription></AlertDialogHeader>
          {batchError ? <p role="alert" className="text-xs text-destructive">{t('credentials.errors.delete')}</p> : null}
          <AlertDialogFooter><AlertDialogCancel disabled={batchPending}>{t('credentials.actions.cancel')}</AlertDialogCancel><AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={batchPending} onClick={(event) => { event.preventDefault(); void removeBatch() }}>{t('credentials.actions.delete')}</AlertDialogAction></AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
