import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Skeleton } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useReconcileUnknownPluginTask, useUnknownPluginTasks } from './plugin-lifecycle-api'

type ReconciliationDraft = {
  taskId: string
  actualUnits: string
  reason: string
}

export function PluginTaskReconciliationPanel() {
  const { t, i18n } = useTranslation()
  const tasks = useUnknownPluginTasks()
  const reconcile = useReconcileUnknownPluginTask()
  const [draft, setDraft] = useState<ReconciliationDraft>()
  const [confirmation, setConfirmation] = useState<ReconciliationDraft>()
  const rows = tasks.data?.pages.flatMap((page) => page.tasks) ?? []
  const actualUnits = draft ? Number(draft.actualUnits) : Number.NaN
  const reasonBytes = draft ? new TextEncoder().encode(draft.reason.trim()).length : 0
  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'

  const submit = () => {
    if (!confirmation) return
    reconcile.mutate({
      taskId: confirmation.taskId,
      actualUnits: Number(confirmation.actualUnits),
      reason: confirmation.reason.trim(),
    }, {
      onSuccess: () => {
        setConfirmation(undefined)
        setDraft(undefined)
      },
    })
  }

  return <section className="rounded-xl border border-warning/30 bg-card p-5" aria-label={t('pluginLifecycle.tasks.title')}>
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 className="text-base font-semibold">{t('pluginLifecycle.tasks.title')}</h2>
        <p className="mt-1 text-xs text-muted-foreground">{t('pluginLifecycle.tasks.description')}</p>
      </div>
      <Button size="sm" variant="light" isDisabled={tasks.isFetching} onPress={() => void tasks.refetch()}>
        {t('pluginLifecycle.refresh')}
      </Button>
    </div>
    <div className="mt-4 space-y-3">
      {tasks.isPending ? <Skeleton className="h-20 rounded-lg" /> : null}
      {tasks.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.tasks.loadError')}</p> : null}
      {tasks.isSuccess && rows.length === 0 ? <p className="rounded-lg border border-dashed border-[var(--hairline)] p-5 text-center text-sm text-muted-foreground">{t('pluginLifecycle.tasks.empty')}</p> : null}
      {rows.map((task) => {
        const editing = draft?.taskId === task.id
        return <article key={task.id} className="rounded-lg border border-[var(--hairline)] p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <strong className="text-sm">{task.pluginId} · {task.model}</strong>
              <p className="mt-1 break-all font-mono text-xs text-muted-foreground">{task.id}</p>
              <p className="mt-1 text-xs text-muted-foreground">{task.failureCode ?? t('pluginLifecycle.tasks.unknownStatus')} · {new Date(task.createdAt * 1000).toLocaleString(locale)}</p>
            </div>
            {!editing ? <Button size="sm" variant="bordered" color="warning" onPress={() => setDraft({ taskId: task.id, actualUnits: '', reason: '' })}>{t('pluginLifecycle.tasks.review')}</Button> : null}
          </div>
          {editing && draft ? <div className="mt-4 grid gap-3 sm:grid-cols-[180px_minmax(0,1fr)_auto] sm:items-end">
            <Input
              type="number"
              min={0}
              max={task.maxUnits}
              step={1}
              label={t('pluginLifecycle.tasks.actualUnits')}
              value={draft.actualUnits}
              onValueChange={(value) => setDraft({ ...draft, actualUnits: value })}
            />
            <Input
              label={t('pluginLifecycle.tasks.reason')}
              value={draft.reason}
              maxLength={256}
              onValueChange={(value) => setDraft({ ...draft, reason: value })}
            />
            <div className="flex gap-2">
              <Button size="sm" variant="light" isDisabled={reconcile.isPending} onPress={() => setDraft(undefined)}>{t('pluginLifecycle.cancel')}</Button>
              <Button
                size="sm"
                color="warning"
                isDisabled={reconcile.isPending || draft.actualUnits === '' || !Number.isSafeInteger(actualUnits) || actualUnits < 0 || actualUnits > task.maxUnits || !draft.reason.trim() || reasonBytes > 256}
                onPress={() => setConfirmation(draft)}
              >{t('pluginLifecycle.tasks.settle')}</Button>
            </div>
          </div> : null}
        </article>
      })}
      {reconcile.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.tasks.reconcileError')}</p> : null}
      {tasks.hasNextPage ? <div className="flex justify-center"><Button size="sm" variant="bordered" isDisabled={tasks.isFetchingNextPage} onPress={() => void tasks.fetchNextPage()}>{t('pluginLifecycle.tasks.more')}</Button></div> : null}
    </div>
    <Modal isOpen={confirmation !== undefined} isDismissable={!reconcile.isPending} isKeyboardDismissDisabled={reconcile.isPending} onOpenChange={(open) => { if (!open && !reconcile.isPending) setConfirmation(undefined) }}>
      <ModalContent>
        <ModalHeader>{t('pluginLifecycle.tasks.confirmTitle')}</ModalHeader>
        <ModalBody>
          <p className="text-sm text-muted-foreground">{t('pluginLifecycle.tasks.confirmDescription')}</p>
          {confirmation ? <div className="space-y-2 rounded-lg border border-[var(--hairline)] p-3 text-sm"><p className="break-all font-mono text-xs">{confirmation.taskId}</p><p>{t('pluginLifecycle.tasks.actualUnitsValue', { count: Number(confirmation.actualUnits) })}</p><p className="break-words text-muted-foreground">{confirmation.reason.trim()}</p></div> : null}
          {reconcile.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.tasks.reconcileError')}</p> : null}
        </ModalBody>
        <ModalFooter>
          <Button variant="light" isDisabled={reconcile.isPending} onPress={() => setConfirmation(undefined)}>{t('pluginLifecycle.cancel')}</Button>
          <Button color="warning" isDisabled={reconcile.isPending} onPress={submit}>{t(reconcile.isPending ? 'pluginLifecycle.saving' : 'pluginLifecycle.tasks.confirm')}</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  </section>
}
