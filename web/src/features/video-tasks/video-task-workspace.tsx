import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import { VideoTaskHistoryPanel } from './video-task-history-panel'
import { VideoTaskRequestPanel } from './video-task-request-panel'
import { VideoTaskResultPanel } from './video-task-result-panel'
import type { VideoTaskRequest } from './video-task-types'
import { useVideoTaskHistory } from './use-video-task-history'
import { useVideoTaskWorkspace } from './use-video-task-workspace'

type VideoTaskWorkspaceProps = {
  renderHeaderControls?: (state: { modeSwitchDisabled: boolean }) => ReactNode
}

/** 复用视频提交、历史恢复与结果展示边界，专页和多模态试炼场共用同一状态机。 */
export function VideoTaskWorkspace(props: VideoTaskWorkspaceProps) {
  const { t } = useTranslation()
  const workspace = useVideoTaskWorkspace()
  const history = useVideoTaskHistory()
  const refreshHistory = history.refresh
  const [submittedRequest, setSubmittedRequest] = useState<VideoTaskRequest>()
  const catalogQuery = useModelCatalog('', {
    ...EMPTY_MODEL_CATALOG_FILTERS,
    protocols: ['xai_video'],
  }, true)
  const models = useMemo(
    () => (catalogQuery.data?.pages.flatMap((page) => page.models) ?? [])
      .filter((item) => item.available_protocols.includes('xai_video')),
    [catalogQuery.data],
  )
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const modelError = catalogQuery.isError || (pricingScope !== undefined && pricingScope !== 'group')

  useEffect(() => {
    if (workspace.taskId) refreshHistory()
  }, [refreshHistory, workspace.taskId])

  const submit = (request: VideoTaskRequest) => {
    setSubmittedRequest(request)
    void workspace.submit(request)
  }

  const startNew = () => {
    setSubmittedRequest(undefined)
    workspace.startNew()
  }

  return (
    <div className="flex min-h-[calc(100dvh-5.75rem)] min-w-0 flex-col gap-2.5">
      <header className="flex flex-col gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1/88 px-3 py-2.5 shadow-subtle backdrop-blur-xl sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('videoTasks.title')}</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">
            {t('videoTasks.subtitle')}
          </p>
        </div>
        {props.renderHeaderControls?.({
          modeSwitchDisabled: workspace.phase === 'submitting' || workspace.canRetrySubmission,
        })}
      </header>

      <div className="grid min-h-0 min-w-0 flex-1 gap-2.5 lg:grid-cols-[20rem_minmax(0,1fr)]">
        <aside className="self-start overflow-hidden rounded-xl border border-dashed border-[var(--hairline)] bg-surface-1/82 shadow-subtle backdrop-blur-xl lg:sticky lg:top-3">
          <VideoTaskRequestPanel
            key={workspace.idempotencyKey}
            busy={workspace.busy}
            credentialReady
            idempotencyKey={workspace.idempotencyKey}
            locked={workspace.requestLocked}
            models={models}
            loadingModels={catalogQuery.isPending}
            loadingMore={catalogQuery.isFetchingNextPage}
            hasMore={Boolean(catalogQuery.hasNextPage)}
            modelError={modelError}
            onLoadMore={() => void catalogQuery.fetchNextPage()}
            onRefreshModels={() => void catalogQuery.refetch()}
            onSubmit={submit}
          />
          <div className="border-t border-[var(--hairline)]">
            <VideoTaskHistoryPanel
              activeTaskId={workspace.taskId}
              error={history.error}
              hasMore={Boolean(history.nextCursor)}
              items={history.items}
              phase={history.phase}
              onLoadMore={history.loadMore}
              onOpen={(taskId) => void workspace.openTask(taskId)}
              onRefresh={history.refresh}
            />
          </div>
        </aside>

        <VideoTaskResultPanel
          busy={workspace.busy}
          canPoll={workspace.canPoll}
          canRetrySubmission={workspace.canRetrySubmission}
          error={workspace.error}
          phase={workspace.phase}
          response={workspace.response}
          taskId={workspace.taskId}
          onNew={startNew}
          onPoll={() => void workspace.poll()}
          onRetrySubmission={() => {
            if (submittedRequest) void workspace.submit(submittedRequest)
          }}
        />
      </div>
    </div>
  )
}
