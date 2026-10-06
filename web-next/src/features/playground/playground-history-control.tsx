import {
  Button,
  Chip,
  Drawer,
  DrawerBody,
  DrawerContent,
  DrawerHeader,
  Modal,
  ModalBody,
  ModalContent,
  ModalFooter,
  ModalHeader,
  Skeleton,
} from '@heroui/react'
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
  if (state === 'saving') return <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" />
  if (state === 'saved') return <CloudCheck className="size-3.5" aria-hidden="true" />
  if (state === 'error' || state === 'conflict' || state === 'limit') {
    return <CloudAlert className="size-3.5" aria-hidden="true" />
  }
  return <History className="size-3.5" aria-hidden="true" />
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
      <Button
        type="button"
        size="sm"
        variant="bordered"
        className="px-2.5"
        title={t(`playground.history.saveState.${props.saveState}`)}
        aria-label={t('playground.history.action')}
        onClick={() => setOpen(true)}
      >
        <SaveStateIcon state={props.saveState} />
        <span className="hidden xl:inline">{t('playground.history.action')}</span>
      </Button>
      <Drawer
        aria-describedby="playground-history-description"
        backdrop="blur"
        classNames={{ base: 'w-full max-h-none sm:max-w-md' }}
        isOpen={open}
        placement="right"
        scrollBehavior="inside"
        onOpenChange={setOpen}
      >
        <DrawerContent>
          {() => (
            <>
              <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
                <h2 className="text-base font-medium text-foreground">{t('playground.history.title')}</h2>
                <p className="text-sm text-muted-foreground" id="playground-history-description">
                  {t('playground.history.description')}
                </p>
              </DrawerHeader>

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
                  <Button isIconOnly aria-label={t('playground.history.retrySave')} size="sm" title={t('playground.history.retrySave')} type="button" variant="light" onClick={props.onRetrySave}>
                    <RotateCcw className="size-3.5" aria-hidden="true" />
                  </Button>
                ) : null}
              </div>

              <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-2">
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
                  <Button type="button" size="sm" variant="bordered" className="mt-4" onClick={() => void historyQuery.refetch()}>
                    <RefreshCw className="size-3.5" aria-hidden="true" />{t('playground.history.retryLoad')}
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
                          {active ? <Chip className="shrink-0 text-primary" size="sm" variant="flat">{t('playground.history.active')}</Chip> : null}
                        </span>
                        <span className="mt-1 flex min-w-0 items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
                          <span className="shrink-0 tabular-nums">{formatTime(conversation.updated_at)}</span>
                          <span aria-hidden="true">/</span>
                          <span className="truncate font-mono">{conversation.models.slice(0, 2).join(', ')}</span>
                          {conversation.models.length > 2 ? <span className="shrink-0">+{conversation.models.length - 2}</span> : null}
                        </span>
                      </button>
                      <Button
                        isIconOnly
                        aria-label={t('playground.history.delete')}
                        className="opacity-70 group-hover:opacity-100"
                        isDisabled={busy}
                        size="sm"
                        title={t('playground.history.delete')}
                        type="button"
                        variant="light"
                        onClick={() => setPendingDelete(conversation)}
                      >
                        <Trash2 className="size-3.5" aria-hidden="true" />
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
              </DrawerBody>
            </>
          )}
        </DrawerContent>
      </Drawer>

      <Modal backdrop="blur" isOpen={pendingRestore !== undefined} onOpenChange={(next) => !next && setPendingRestore(undefined)}>
        <ModalContent>
          {() => (
            <>
              <ModalHeader className="grid gap-1.5">
                <h2 className="text-base font-semibold">{t('playground.history.restoreTitle')}</h2>
                <p className="text-sm leading-5 font-normal text-muted-foreground">{t('playground.history.restoreDescription')}</p>
              </ModalHeader>
              <ModalBody className="gap-1.5" />
              <ModalFooter>
                <Button variant="light" onPress={() => setPendingRestore(undefined)}>{t('playground.history.keepCurrent')}</Button>
                <Button color="primary" onPress={() => pendingRestore && void restore(pendingRestore)}>
                  {t('playground.history.restoreAction')}
                </Button>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>

      <Modal backdrop="blur" isOpen={pendingDelete !== undefined} onOpenChange={(next) => !next && setPendingDelete(undefined)}>
        <ModalContent>
          {() => (
            <>
              <ModalHeader className="grid gap-1.5">
                <h2 className="text-base font-semibold">{t('playground.history.deleteTitle')}</h2>
                <p className="text-sm leading-5 font-normal text-muted-foreground">{t('playground.history.deleteDescription')}</p>
              </ModalHeader>
              <ModalBody className="gap-1.5" />
              <ModalFooter>
                <Button variant="light" onPress={() => setPendingDelete(undefined)}>{t('playground.history.keepHistory')}</Button>
                <Button color="danger" variant="flat" onPress={() => pendingDelete && void remove(pendingDelete)}>
                  {t('playground.history.deleteAction')}
                </Button>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </>
  )
}
