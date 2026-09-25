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
  useAdminChannelCredentialCatalog,
  useDeleteAdminCredential,
  useUpdateAdminCredential,
} from './credential-api'
import { updateCredentialStatusRequest } from './credential-form-model'
import { CredentialItem } from './credential-item'
import { supportsSparkShadow } from './credential-model'

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
  const sparkChannel = supportsSparkShadow(props.channel)
  const deleteCatalogQuery = useAdminChannelCredentialCatalog(
    props.channel.id,
    deleting !== undefined && sparkChannel,
  )
  const deleteCatalogReady = !sparkChannel || deleteCatalogQuery.isSuccess
  const deleteCatalog = sparkChannel ? deleteCatalogQuery.data ?? [] : props.credentials
  const shadowCount = deleteCatalog.filter((item) => item.parent_id === deleting?.id).length

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

        {props.loading ? (
          <div className="grid gap-2 py-3" aria-label={t('credentials.list.loading')}>{[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-xl" />)}</div>
        ) : props.loadError ? (
          <div role="alert" className="my-3 rounded-xl border border-destructive/25 bg-destructive/8 p-4"><p className="text-xs text-destructive">{t('credentials.errors.load')}</p><Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => props.onRefresh()}>{t('credentials.actions.retry')}</Button></div>
        ) : props.credentials.length === 0 ? (
          <div className="grid min-h-48 place-items-center border-b border-[var(--hairline)] text-center"><div><KeyRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{t('credentials.empty.title')}</h3><p className="mt-1 max-w-sm text-xs leading-5 text-muted-foreground">{t('credentials.empty.description')}</p></div></div>
        ) : (
          <div className="divide-y divide-[var(--hairline)] border-b border-[var(--hairline)]">
            {props.credentials.map((credential) => (
              <CredentialItem
                key={credential.id}
                channelType={props.channel.type}
                credential={credential}
                parent={credential.parent_id ? props.credentials.find((item) => item.id === credential.parent_id) : undefined}
                language={i18n.language}
                pending={updateMutation.isPending && updateMutation.variables?.credentialId === credential.id}
                active={props.active}
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
    </>
  )
}
