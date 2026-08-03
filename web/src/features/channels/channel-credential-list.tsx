import type { MouseEvent } from 'react'
import { useCallback, useMemo, useState } from 'react'
import { KeyRound, LoaderCircle, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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
import { Button, buttonVariants } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import {
  useAdminChannelCredentials,
  useDeleteAdminCredential,
  useUpdateAdminCredential,
} from './channel-api'
import { ChannelCredentialItem } from './channel-credential-item'
import {
  updateCredentialScheduleRequest,
  updateCredentialStatusRequest,
  type CredentialSchedule,
} from './channel-credential-model'

type ChannelCredentialListProps = {
  channel?: AdminChannel
  open: boolean
}

/** 管理凭据列表的加载、状态更新、调度编辑与安全删除流程。 */
export function ChannelCredentialList({ channel, open }: ChannelCredentialListProps) {
  const { i18n, t } = useTranslation()
  const credentialsQuery = useAdminChannelCredentials(channel?.id)
  const updateMutation = useUpdateAdminCredential()
  const deleteMutation = useDeleteAdminCredential()
  const [deleting, setDeleting] = useState<AdminCredential>()
  const credentials = useMemo(
    () => credentialsQuery.data?.pages.flatMap((page) => page.credentials) ?? [],
    [credentialsQuery.data],
  )
  const { refetch: refetchCredentials } = credentialsQuery
  const refreshCredentials = useCallback(async () => {
    await refetchCredentials()
  }, [refetchCredentials])

  const updateCredential = async (credential: AdminCredential, body: ReturnType<typeof updateCredentialStatusRequest>) => {
    if (!channel || !body) return false
    try {
      await updateMutation.mutateAsync({ channelId: channel.id, credentialId: credential.id, body })
      return true
    } catch {
      // 行内仅呈现固定错误，服务端诊断和敏感字段保持在安全边界内。
      return false
    }
  }

  const toggleCredential = (credential: AdminCredential) => updateCredential(
    credential,
    updateCredentialStatusRequest(credential, credential.status === 'enabled' ? 'disabled' : 'enabled'),
  )
  const updateSchedule = (credential: AdminCredential, schedule: CredentialSchedule) => updateCredential(
    credential,
    updateCredentialScheduleRequest(credential, schedule),
  )

  const deleteCredential = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!channel || !deleting) return
    try {
      await deleteMutation.mutateAsync({ channelId: channel.id, credentialId: deleting.id })
      setDeleting(undefined)
    } catch {
      // 删除失败时保留确认框，用户可在不丢失上下文的情况下重试。
    }
  }

  return (
    <>
      <section className="p-4" aria-labelledby="channel-credential-list-title">
        <div className="flex items-center justify-between gap-3">
          <div><h3 id="channel-credential-list-title" className="text-xs font-semibold">{t('channels.credentials.listTitle')}</h3><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.credentials.count', { count: credentials.length })}</p></div>
          <Button type="button" size="icon-sm" variant="ghost" aria-label={t('channels.actions.refreshCredentials')} title={t('channels.actions.refreshCredentials')} disabled={credentialsQuery.isFetching} onClick={() => credentialsQuery.refetch()}><RefreshCw className={credentialsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" /></Button>
        </div>

        {credentialsQuery.isPending ? (
          <div className="mt-3 grid gap-2" aria-label={t('channels.credentials.loading')}>{[0, 1, 2].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}</div>
        ) : credentialsQuery.isError && !credentialsQuery.data ? (
          <div role="alert" className="mt-3 rounded-lg border border-destructive/25 bg-destructive/8 p-3"><p className="text-xs text-destructive">{t('channels.credentials.loadFailed')}</p><Button type="button" size="sm" variant="secondary" className="mt-2" onClick={() => credentialsQuery.refetch()}>{t('channels.actions.retry')}</Button></div>
        ) : credentials.length === 0 ? (
          <div className="mt-3 grid min-h-32 place-items-center border-y border-[var(--hairline)] text-center"><div><KeyRound className="mx-auto size-4 text-muted-foreground" aria-hidden="true" /><p className="mt-2 text-xs font-medium">{t('channels.credentials.emptyTitle')}</p><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.credentials.emptyBody')}</p></div></div>
        ) : (
          <div className="mt-3 divide-y divide-[var(--hairline)] border-y border-[var(--hairline)]">
            {credentials.map((credential) => (
              <ChannelCredentialItem
                key={credential.id}
                channelId={channel?.id ?? credential.channel_id}
                credential={credential}
                language={i18n.language}
                pending={updateMutation.isPending && updateMutation.variables?.credentialId === credential.id}
                sheetOpen={open}
                onToggle={(item) => { void toggleCredential(item) }}
                onSchedule={updateSchedule}
                onDelete={setDeleting}
                onRefresh={refreshCredentials}
              />
            ))}
          </div>
        )}

        {credentialsQuery.isRefetchError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('channels.credentials.refreshFailed')}</p> : null}
        {updateMutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('channels.credentials.updateFailed')}</p> : null}
        {credentialsQuery.hasNextPage ? <Button type="button" variant="secondary" className="mt-3 w-full" disabled={credentialsQuery.isFetchingNextPage} onClick={() => credentialsQuery.fetchNextPage()}>{credentialsQuery.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('channels.actions.loadMore')}</Button> : null}
      </section>

      <AlertDialog open={deleting !== undefined} onOpenChange={(nextOpen) => { if (!nextOpen && !deleteMutation.isPending) setDeleting(undefined) }}>
        <AlertDialogContent>
          <AlertDialogHeader><AlertDialogTitle>{t('channels.credentials.deleteTitle')}</AlertDialogTitle><AlertDialogDescription>{t('channels.credentials.deleteDescription', { id: deleting?.id ?? '' })}</AlertDialogDescription></AlertDialogHeader>
          {deleteMutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('channels.credentials.deleteFailed')}</p> : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>{t('channels.actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={deleteMutation.isPending} onClick={deleteCredential}>{deleteMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}{t('channels.credentials.delete')}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
