import {
  CloudAlert,
  CloudCheck,
  History,
  LoaderCircle,
  MessageSquareText,
  RefreshCw,
  RotateCcw,
  Trash2,
} from 'lucide-react'
import { useState } from 'react'
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
import { Button } from '@/components/ui/button'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/lib/utils'
import {
  type PlaygroundConversationResponse,
  type PlaygroundConversationSummary,
  useDeletePlaygroundConversation,
  usePlaygroundHistoryList,
  useReadPlaygroundConversation,
} from './playground-history-api'
import type { PlaygroundHistorySaveState } from './use-playground-history'

type PlaygroundHistoryControlProps = {
  activeConversationId?: string
  hasConversation: boolean
  saveState: PlaygroundHistorySaveState
  streaming: boolean
  onActiveDeleted: () => void
  onRestore: (conversation: PlaygroundConversationResponse) => void
  onRetrySave: () => void
}

function SaveStateIcon({ state }: { state: PlaygroundHistorySaveState }) {
  if (state === 'saving') return <LoaderCircle className="animate-spin" aria-hidden="true" />
  if (state === 'saved') return <CloudCheck aria-hidden="true" />
  if (state === 'error' || state === 'conflict' || state === 'limit') {
    return <CloudAlert aria-hidden="true" />
  }
  return <History aria-hidden="true" />
}

export function PlaygroundHistoryControl(props: PlaygroundHistoryControlProps) {
  const { t, i18n } = useTranslation()
  const [open, setOpen] = useState(false)
  const [pendingRestore, setPendingRestore] = useState<PlaygroundConversationSummary>()
  const [pendingDelete, setPendingDelete] = useState<PlaygroundConversationSummary>()
  const historyQuery = usePlaygroundHistoryList(open)
  const readMutation = useReadPlaygroundConversation()
  const deleteMutation = useDeletePlaygroundConversation()
  const busy = props.streaming || readMutation.isPending || deleteMutation.isPending

  const formatTime = (value: number) => new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value * 1000)

  const restore = async (summary: PlaygroundConversationSummary) => {
    try {
      const conversation = await readMutation.mutateAsync(summary.conversation_id)
      props.onRestore(conversation)
      setOpen(false)
    } catch {
      // Mutation 状态在抽屉中提供稳定错误与重试入口。
    }
  }

  const requestRestore = (summary: PlaygroundConversationSummary) => {
    const isActive = summary.conversation_id === props.activeConversationId
    if (isActive && props.saveState !== 'conflict') {
      setOpen(false)
      return
    }
    if (props.hasConversation) setPendingRestore(summary)
    else void restore(summary)
  }

  const remove = async (summary: PlaygroundConversationSummary) => {
    try {
      await deleteMutation.mutateAsync(summary.conversation_id)
      if (summary.conversation_id === props.activeConversationId) props.onActiveDeleted()
      else if (props.saveState === 'limit') props.onRetrySave()
    } catch {
      // 删除失败保留原列表，允许用户关闭确认框后重试。
    }
  }

  const canRetrySave = props.saveState === 'error' || props.saveState === 'limit'

  return (
    <>
      <span className="sr-only" role="status" aria-live="polite">
        {t(`playground.history.saveState.${props.saveState}`)}
      </span>
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetTrigger asChild>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="px-2.5"
            title={t(`playground.history.saveState.${props.saveState}`)}
            aria-label={t('playground.history.action')}
          >
            <SaveStateIcon state={props.saveState} />
            <span className="hidden xl:inline">{t('playground.history.action')}</span>
          </Button>
        </SheetTrigger>
        <SheetContent
          className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-md"
          aria-describedby="playground-history-description"
        >
          <SheetHeader className="border-b border-[var(--hairline)] pr-12">
            <SheetTitle>{t('playground.history.title')}</SheetTitle>
            <SheetDescription id="playground-history-description">
              {t('playground.history.description')}
            </SheetDescription>
          </SheetHeader>

          <div className="flex min-h-12 items-center gap-2 border-b border-[var(--hairline)] px-4 py-2.5">
            <span className={cn(
              'grid size-7 shrink-0 place-items-center rounded-md bg-surface-2 text-muted-foreground [&_svg]:size-3.5',
              props.saveState === 'saved' && 'text-success',
              ['error', 'conflict', 'limit'].includes(props.saveState) && 'text-destructive',
            )}>
              <SaveStateIcon state={props.saveState} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium">{t(`playground.history.saveState.${props.saveState}`)}</p>
              <p className="truncate text-[0.6875rem] text-muted-foreground">
                {t(`playground.history.saveDetail.${props.saveState}`)}
              </p>
            </div>
            {canRetrySave ? (
              <Button type="button" size="icon-sm" variant="ghost" title={t('playground.history.retrySave')} onClick={props.onRetrySave}>
                <RotateCcw aria-hidden="true" />
                <span className="sr-only">{t('playground.history.retrySave')}</span>
              </Button>
            ) : null}
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {historyQuery.isPending ? (
              <div className="grid gap-1" aria-label={t('playground.history.loading')}>
                {Array.from({ length: 5 }, (_, index) => (
                  <div key={index} className="grid gap-2 px-3 py-3">
                    <Skeleton className="h-4 w-3/4" />
                    <Skeleton className="h-3 w-1/2" />
                  </div>
                ))}
              </div>
            ) : historyQuery.isError ? (
              <div className="grid min-h-56 place-items-center px-5 text-center">
                <div className="max-w-xs">
                  <CloudAlert className="mx-auto size-5 text-destructive" aria-hidden="true" />
                  <h3 className="mt-3 text-sm font-semibold">{t('playground.history.loadErrorTitle')}</h3>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('playground.history.loadErrorBody')}</p>
                  <Button type="button" size="sm" variant="secondary" className="mt-4" onClick={() => void historyQuery.refetch()}>
                    <RefreshCw aria-hidden="true" />{t('playground.history.retryLoad')}
                  </Button>
                </div>
              </div>
            ) : historyQuery.data.conversations.length === 0 ? (
              <div className="grid min-h-56 place-items-center px-5 text-center">
                <div className="max-w-xs">
                  <MessageSquareText className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
                  <h3 className="mt-3 text-sm font-semibold">{t('playground.history.emptyTitle')}</h3>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('playground.history.emptyBody')}</p>
                </div>
              </div>
            ) : (
              <div className="grid gap-1">
                {historyQuery.data.conversations.map((conversation) => {
                  const active = conversation.conversation_id === props.activeConversationId
                  return (
                    <div
                      key={conversation.conversation_id}
                      className={cn(
                        'group grid min-w-0 grid-cols-[minmax(0,1fr)_2rem] items-center rounded-lg',
                        active ? 'bg-primary/8' : 'hover:bg-surface-2/70',
                      )}
                    >
                      <button
                        type="button"
                        className="min-w-0 px-3 py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60"
                        disabled={busy}
                        onClick={() => requestRestore(conversation)}
                      >
                        <span className="flex min-w-0 items-center gap-2">
                          <span className="truncate text-sm font-medium">{conversation.title}</span>
                          {active ? <Badge className="shrink-0 text-primary">{t('playground.history.active')}</Badge> : null}
                        </span>
                        <span className="mt-1 flex min-w-0 items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
                          <span className="shrink-0 tabular-nums">{formatTime(conversation.updated_at)}</span>
                          <span aria-hidden="true">/</span>
                          <span className="truncate font-mono">{conversation.models.slice(0, 2).join(', ')}</span>
                          {conversation.models.length > 2 ? <span className="shrink-0">+{conversation.models.length - 2}</span> : null}
                        </span>
                      </button>
                      <Button
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        className="opacity-70 group-hover:opacity-100"
                        disabled={busy}
                        title={t('playground.history.delete')}
                        onClick={() => setPendingDelete(conversation)}
                      >
                        <Trash2 aria-hidden="true" />
                        <span className="sr-only">{t('playground.history.delete')}</span>
                      </Button>
                    </div>
                  )
                })}
              </div>
            )}
            {readMutation.isError ? (
              <p role="alert" className="m-2 rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">
                {t('playground.history.readError')}
              </p>
            ) : null}
            {deleteMutation.isError ? (
              <p role="alert" className="m-2 rounded-md bg-destructive/8 px-3 py-2 text-xs text-destructive">
                {t('playground.history.deleteError')}
              </p>
            ) : null}
          </div>
        </SheetContent>
      </Sheet>

      <AlertDialog open={pendingRestore !== undefined} onOpenChange={(next) => !next && setPendingRestore(undefined)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('playground.history.restoreTitle')}</AlertDialogTitle>
            <AlertDialogDescription>{t('playground.history.restoreDescription')}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('playground.history.keepCurrent')}</AlertDialogCancel>
            <AlertDialogAction onClick={() => pendingRestore && void restore(pendingRestore)}>
              {t('playground.history.restoreAction')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={pendingDelete !== undefined} onOpenChange={(next) => !next && setPendingDelete(undefined)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t('playground.history.deleteTitle')}</AlertDialogTitle>
            <AlertDialogDescription>{t('playground.history.deleteDescription')}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t('playground.history.keepHistory')}</AlertDialogCancel>
            <AlertDialogAction className="bg-destructive text-destructive-foreground hover:bg-destructive/90" onClick={() => pendingDelete && void remove(pendingDelete)}>
              {t('playground.history.deleteAction')}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
