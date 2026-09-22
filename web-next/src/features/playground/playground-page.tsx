import { Button } from '@heroui/react'
import { PanelRightClose } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { VideoTaskWorkspace } from '@/features/video-tasks/video-task-workspace'
import { cn } from '@/lib/utils'
import { PlaygroundComparisonPanel } from './playground-comparison-panel'
import { PlaygroundModelPicker } from './playground-model-picker'
import { PlaygroundModeSwitcher, type PlaygroundMode } from './playground-mode-switcher'
import { PlaygroundPageHeader } from './playground-page-header'
import { PlaygroundHistoryControl } from './playground-history-control'
import { restorePlaygroundSessions } from './playground-history-snapshot'
import { PlaygroundSettings } from './playground-settings'
import { PlaygroundShareControl } from './playground-share-control'
import type { PlaygroundDisplayStatus } from './playground-status'
import { usePlaygroundComparison } from './use-playground-comparison'
import { usePlaygroundConfiguration } from './use-playground-configuration'
import { usePlaygroundHistory } from './use-playground-history'

export function PlaygroundPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<PlaygroundMode>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const configuration = usePlaygroundConfiguration()
  const comparison = usePlaygroundComparison(
    configuration.sharedSettings,
    configuration.selectedModels,
    configuration.protocolByModel,
  )
  const history = usePlaygroundHistory(comparison.sessions, comparison.streaming)

  const clearConversation = () => {
    history.startNewConversation()
    comparison.clear()
  }

  const restoreConversation = (conversation: Parameters<typeof history.bindRestoredConversation>[0]) => {
    configuration.restoreConversationConfiguration(
      conversation.sessions.map((session) => session.model),
    )
    comparison.restore(restorePlaygroundSessions(conversation.sessions))
    history.bindRestoredConversation(conversation)
  }

  const changeModels = (models: string[], comparisonEnabled: boolean) => {
    configuration.setSelectedModels(models, comparisonEnabled)
  }

  const renderModelPicker = (compact = false) => (
    <PlaygroundModelPicker
      compact={compact}
      values={configuration.selectedModels}
      comparisonEnabled={configuration.comparisonEnabled}
      search={configuration.modelSearchDraft}
      models={configuration.models}
      protocolByModel={configuration.protocolByModel}
      loading={configuration.catalogQuery.isPending}
      error={configuration.catalogInvalid}
      loadingMore={configuration.catalogQuery.isFetchingNextPage}
      hasMore={Boolean(configuration.catalogQuery.hasNextPage)}
      locked={comparison.streaming || history.saving}
      onChange={changeModels}
      onLoadMore={() => void configuration.catalogQuery.fetchNextPage()}
      onRefresh={() => void configuration.catalogQuery.refetch()}
      onSearch={configuration.setModelSearchDraft}
    />
  )

  const renderSettings = (idPrefix: string) => (
    <PlaygroundSettings
      idPrefix={idPrefix}
      systemPrompt={configuration.systemPrompt}
      temperatureEnabled={configuration.temperatureEnabled}
      temperature={configuration.temperature}
      maxOutputTokensEnabled={configuration.maxOutputTokensEnabled}
      maxOutputTokens={configuration.maxOutputTokens}
      onSystemPromptChange={configuration.setSystemPrompt}
      onTemperatureEnabledChange={configuration.setTemperatureEnabled}
      onTemperatureChange={configuration.setTemperature}
      onMaxOutputTokensEnabledChange={configuration.setMaxOutputTokensEnabled}
      onMaxOutputTokensChange={configuration.setMaxOutputTokens}
    />
  )

  let status: PlaygroundDisplayStatus = comparison.overallState === 'idle'
    ? 'ready'
    : comparison.overallState
  if (!comparison.streaming) {
    if (configuration.catalogQuery.isPending) status = 'loadingModels'
    else if (
      configuration.catalogInvalid
      || configuration.selectedModels.length === 0
      || !configuration.selectedProtocolsReady
    ) {
      status = 'modelUnavailable'
    }
  }

  const canSend = Boolean(
    configuration.selectedModels.length > 0
    && configuration.selectedProtocolsReady
    && !configuration.catalogInvalid
    && comparison.draft.trim()
    && !comparison.streaming
    && !history.saving,
  )

  const changeMode = (nextMode: PlaygroundMode) => {
    setMode(nextMode)
  }

  if (mode === 'video') {
    return (
      <VideoTaskWorkspace
        renderHeaderControls={({ modeSwitchDisabled }) => (
          <PlaygroundModeSwitcher
            disabled={modeSwitchDisabled}
            mode={mode}
            onChange={changeMode}
          />
        )}
      />
    )
  }

  return (
    <div className="flex h-[calc(100dvh-5.75rem)] min-h-[36rem] min-w-0 flex-col gap-2.5">
      <PlaygroundPageHeader
        clearDisabled={!comparison.hasConversation && !comparison.draft}
        historyControl={(
          <PlaygroundHistoryControl
            activeConversationId={history.activeConversationId}
            hasConversation={comparison.hasConversation}
            saveState={history.saveState}
            streaming={comparison.streaming}
            onActiveDeleted={clearConversation}
            onRestore={restoreConversation}
            onRetrySave={history.retry}
          />
        )}
        mobileSettings={renderSettings('playground-mobile')}
        modeControl={(
          <PlaygroundModeSwitcher
            disabled={comparison.streaming}
            mode={mode}
            onChange={changeMode}
          />
        )}
        modelCount={configuration.selectedModels.length}
        shareControl={(
          <PlaygroundShareControl
            sessions={comparison.sessions}
            streaming={comparison.streaming}
          />
        )}
        status={status}
        desktopSettingsOpen={settingsOpen}
        onDesktopSettingsToggle={() => setSettingsOpen((open) => !open)}
        onClear={clearConversation}
      />
      <div className="relative flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div
          className={cn(
            'min-h-0 min-w-0 flex-1 transition-[padding] duration-[250ms] ease-[var(--ease-ai-out)]',
            settingsOpen && 'lg:pr-[18.125rem]',
          )}
        >
          <PlaygroundComparisonPanel
            canSend={canSend}
            draft={comparison.draft}
            modelPicker={renderModelPicker(true)}
            sessions={comparison.sessions}
            status={status}
            streaming={comparison.streaming}
            onDraftChange={comparison.setDraft}
            onRetry={comparison.retry}
            onSend={comparison.send}
            onStopAll={comparison.stopAll}
            onStopModel={comparison.stopModel}
          />
        </div>
        <aside
          aria-hidden={!settingsOpen}
          {...(!settingsOpen ? { inert: true } : {})}
          className={cn(
            'playground-settings-panel absolute inset-y-0 right-0 z-10 hidden w-[17.5rem] overflow-y-auto border-l border-[var(--hairline)] bg-surface-1/96 backdrop-blur-xl lg:block',
            !settingsOpen && 'pointer-events-none',
          )}
          data-open={settingsOpen}
        >
          <div className="sticky top-0 z-10 flex h-11 items-center justify-between border-b border-[var(--hairline)] bg-surface-1/92 px-3 backdrop-blur-xl">
            <span className="text-xs font-semibold">{t('playground.settings.title')}</span>
            <Button
              isIconOnly
              aria-label={t('playground.settings.collapse')}
              className="size-6 min-w-6"
              size="sm"
              title={t('playground.settings.collapse')}
              type="button"
              variant="light"
              onClick={() => setSettingsOpen(false)}
            >
              <PanelRightClose className="size-3" aria-hidden="true" />
            </Button>
          </div>
          {renderSettings('playground-desktop')}
        </aside>
      </div>
    </div>
  )
}
