import type { FormEvent, MouseEvent } from 'react'
import { useMemo, useState } from 'react'
import { Eye, EyeOff, KeyRound, LoaderCircle, Plus, Power, RefreshCw, Trash2 } from 'lucide-react'
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
import { Badge } from '@/components/ui/badge'
import { Button, buttonVariants } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  useAdminChannelCredentials,
  useCreateAdminCredential,
  useDeleteAdminCredential,
  useUpdateAdminCredential,
} from './channel-api'
import {
  createApiKeyRequest,
  credentialRuntimeState,
  isValidApiKey,
  updateCredentialStatusRequest,
} from './channel-credential-model'

type ChannelCredentialsSheetProps = {
  channel?: AdminChannel
  open: boolean
  onOpenChange: (open: boolean) => void
}

const stateStyles = {
  available: 'bg-success/10 text-success',
  autoDisabled: 'bg-destructive/10 text-destructive',
  cooling: 'bg-warning/10 text-warning',
  disabled: 'bg-surface-2 text-muted-foreground',
  paused: 'bg-surface-2 text-muted-foreground',
  pending: 'bg-info/10 text-info',
} as const

export function ChannelCredentialsSheet({
  channel,
  open,
  onOpenChange,
}: ChannelCredentialsSheetProps) {
  const { i18n, t } = useTranslation()
  const credentialsQuery = useAdminChannelCredentials(channel?.id)
  const createMutation = useCreateAdminCredential()
  const updateMutation = useUpdateAdminCredential()
  const deleteMutation = useDeleteAdminCredential()
  const [secret, setSecret] = useState('')
  const [showSecret, setShowSecret] = useState(false)
  const [invalidSecret, setInvalidSecret] = useState(false)
  const [deleting, setDeleting] = useState<AdminCredential>()
  const credentials = useMemo(
    () => credentialsQuery.data?.pages.flatMap((page) => page.credentials) ?? [],
    [credentialsQuery.data],
  )

  const addCredential = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!channel || !isValidApiKey(secret)) {
      setInvalidSecret(true)
      return
    }
    try {
      await createMutation.mutateAsync({
        channelId: channel.id,
        body: createApiKeyRequest(secret),
      })
      setSecret('')
      setShowSecret(false)
      setInvalidSecret(false)
    } catch {
      // Mutation 状态负责呈现固定错误，明文保持在输入框中供用户确认后重试。
    }
  }

  const toggleCredential = async (credential: AdminCredential) => {
    if (!channel) return
    const body = updateCredentialStatusRequest(
      credential,
      credential.status === 'enabled' ? 'disabled' : 'enabled',
    )
    if (!body) return
    try {
      await updateMutation.mutateAsync({
        channelId: channel.id,
        credentialId: credential.id,
        body,
      })
    } catch {
      // 行内错误状态由 mutation 统一呈现，避免泄露服务端诊断。
    }
  }

  const deleteCredential = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!channel || !deleting) return
    try {
      await deleteMutation.mutateAsync({
        channelId: channel.id,
        credentialId: deleting.id,
      })
      setDeleting(undefined)
    } catch {
      // 删除失败时保持确认框，允许用户在不丢失上下文的情况下重试。
    }
  }

  return (
    <>
      <Sheet open={open} onOpenChange={(nextOpen) => {
        if (!nextOpen && !createMutation.isPending && !updateMutation.isPending) {
          setSecret('')
          setShowSecret(false)
          setInvalidSecret(false)
          onOpenChange(false)
        }
      }}>
        <SheetContent
          className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl"
          aria-describedby="channel-credentials-description"
        >
          <SheetHeader className="border-b border-[var(--hairline)] pr-12">
            <SheetTitle>{t('channels.credentials.title')}</SheetTitle>
            <SheetDescription id="channel-credentials-description">
              {t('channels.credentials.description', { name: channel?.name ?? '' })}
            </SheetDescription>
          </SheetHeader>

          <div className="flex-1 overflow-y-auto">
            <form className="grid gap-3 border-b border-[var(--hairline)] p-4" onSubmit={addCredential} noValidate>
              <div>
                <h3 className="text-xs font-semibold">{t('channels.credentials.addTitle')}</h3>
                <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
                  {t('channels.credentials.addHint')}
                </p>
              </div>
              <div className="grid gap-1.5">
                <Label htmlFor="channel-credential-secret">{t('channels.credentials.apiKey')}</Label>
                <div className="flex gap-2">
                  <div className="relative min-w-0 flex-1">
                    <Input
                      id="channel-credential-secret"
                      type={showSecret ? 'text' : 'password'}
                      autoComplete="off"
                      autoCapitalize="none"
                      autoCorrect="off"
                      spellCheck={false}
                      className="pr-10 font-mono"
                      value={secret}
                      aria-invalid={invalidSecret}
                      onChange={(event) => {
                        setSecret(event.target.value)
                        setInvalidSecret(false)
                        createMutation.reset()
                      }}
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground"
                      aria-label={t(showSecret ? 'channels.actions.hideApiKey' : 'channels.actions.showApiKey')}
                      title={t(showSecret ? 'channels.actions.hideApiKey' : 'channels.actions.showApiKey')}
                      aria-pressed={showSecret}
                      onClick={() => setShowSecret((current) => !current)}
                    >
                      {showSecret ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
                    </Button>
                  </div>
                  <Button type="submit" disabled={createMutation.isPending}>
                    {createMutation.isPending
                      ? <LoaderCircle className="animate-spin" aria-hidden="true" />
                      : <Plus aria-hidden="true" />}
                    {t('channels.credentials.add')}
                  </Button>
                </div>
                {invalidSecret ? <p className="text-xs text-destructive">{t('channels.validation.apiKey')}</p> : null}
                {createMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('channels.credentials.addFailed')}</p> : null}
              </div>
            </form>

            <section className="p-4" aria-labelledby="channel-credential-list-title">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h3 id="channel-credential-list-title" className="text-xs font-semibold">
                    {t('channels.credentials.listTitle')}
                  </h3>
                  <p className="mt-1 text-[0.6875rem] text-muted-foreground">
                    {t('channels.credentials.count', { count: credentials.length })}
                  </p>
                </div>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={t('channels.actions.refreshCredentials')}
                  title={t('channels.actions.refreshCredentials')}
                  disabled={credentialsQuery.isFetching}
                  onClick={() => credentialsQuery.refetch()}
                >
                  <RefreshCw className={credentialsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
                </Button>
              </div>

              {credentialsQuery.isPending ? (
                <div className="mt-3 grid gap-2" aria-label={t('channels.credentials.loading')}>
                  {[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}
                </div>
              ) : credentialsQuery.isError ? (
                <div role="alert" className="mt-3 rounded-lg border border-destructive/25 bg-destructive/8 p-3">
                  <p className="text-xs text-destructive">{t('channels.credentials.loadFailed')}</p>
                  <Button type="button" size="sm" variant="secondary" className="mt-2" onClick={() => credentialsQuery.refetch()}>
                    {t('channels.actions.retry')}
                  </Button>
                </div>
              ) : credentials.length === 0 ? (
                <div className="mt-3 grid min-h-32 place-items-center border-y border-[var(--hairline)] text-center">
                  <div>
                    <KeyRound className="mx-auto size-4 text-muted-foreground" aria-hidden="true" />
                    <p className="mt-2 text-xs font-medium">{t('channels.credentials.emptyTitle')}</p>
                    <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.credentials.emptyBody')}</p>
                  </div>
                </div>
              ) : (
                <div className="mt-3 divide-y divide-[var(--hairline)] border-y border-[var(--hairline)]">
                  {credentials.map((credential) => {
                    const runtimeState = credentialRuntimeState(credential)
                    const toggling = updateMutation.isPending
                      && updateMutation.variables?.credentialId === credential.id
                    return (
                      <article key={credential.id} className="flex items-center gap-3 py-3">
                        <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
                          <KeyRound className="size-4" aria-hidden="true" />
                        </div>
                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <h4 className="text-xs font-medium">{t('channels.credentials.itemName', { id: credential.id })}</h4>
                            <Badge className={cn('border-transparent', stateStyles[runtimeState])}>
                              {t(`channels.credentials.state.${runtimeState}`)}
                            </Badge>
                          </div>
                          <p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
                            {credential.last_used_at === null
                              ? t('channels.credentials.neverUsed')
                              : t('channels.credentials.lastUsed', {
                                  value: formatTimestamp(credential.last_used_at, i18n.language),
                                })}
                          </p>
                        </div>
                        <div className="flex shrink-0 items-center gap-1">
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                type="button"
                                size="icon-sm"
                                variant="ghost"
                                disabled={toggling || updateCredentialStatusRequest(credential, 'enabled') === undefined}
                                aria-label={t(credential.status === 'enabled'
                                  ? 'channels.credentials.disable'
                                  : 'channels.credentials.enable')}
                                onClick={() => toggleCredential(credential)}
                              >
                                {toggling
                                  ? <LoaderCircle className="animate-spin" aria-hidden="true" />
                                  : <Power aria-hidden="true" />}
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>{t(credential.status === 'enabled'
                              ? 'channels.credentials.disable'
                              : 'channels.credentials.enable')}</TooltipContent>
                          </Tooltip>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                type="button"
                                size="icon-sm"
                                variant="ghost"
                                className="text-muted-foreground hover:text-destructive"
                                aria-label={t('channels.credentials.delete')}
                                onClick={() => setDeleting(credential)}
                              >
                                <Trash2 aria-hidden="true" />
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>{t('channels.credentials.delete')}</TooltipContent>
                          </Tooltip>
                        </div>
                      </article>
                    )
                  })}
                </div>
              )}

              {updateMutation.isError ? (
                <p role="alert" className="mt-3 text-xs text-destructive">{t('channels.credentials.updateFailed')}</p>
              ) : null}
              {credentialsQuery.hasNextPage ? (
                <Button
                  type="button"
                  variant="secondary"
                  className="mt-3 w-full"
                  disabled={credentialsQuery.isFetchingNextPage}
                  onClick={() => credentialsQuery.fetchNextPage()}
                >
                  {credentialsQuery.isFetchingNextPage
                    ? <LoaderCircle className="animate-spin" aria-hidden="true" />
                    : null}
                  {t('channels.actions.loadMore')}
                </Button>
              ) : null}
            </section>
          </div>
        </SheetContent>
      </Sheet>

      <AlertDialog open={deleting !== undefined} onOpenChange={(nextOpen) => {
        if (!nextOpen && !deleteMutation.isPending) setDeleting(undefined)
      }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('channels.credentials.deleteTitle')}</AlertDialogTitle>
            <AlertDialogDescription>
              {t('channels.credentials.deleteDescription', { id: deleting?.id ?? '' })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          {deleteMutation.isError ? (
            <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
              {t('channels.credentials.deleteFailed')}
            </p>
          ) : null}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>{t('channels.actions.cancel')}</AlertDialogCancel>
            <AlertDialogAction
              className={buttonVariants({ variant: 'destructive' })}
              disabled={deleteMutation.isPending}
              onClick={deleteCredential}
            >
              {deleteMutation.isPending
                ? <LoaderCircle className="animate-spin" aria-hidden="true" />
                : <Trash2 aria-hidden="true" />}
              {t('channels.credentials.delete')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}

function formatTimestamp(timestamp: number, language: string) {
  return new Intl.DateTimeFormat(language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(timestamp * 1000)
}
