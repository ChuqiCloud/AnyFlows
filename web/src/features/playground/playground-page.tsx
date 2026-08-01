import { PlaygroundComparisonPanel } from './playground-comparison-panel'
import { PlaygroundModelPicker } from './playground-model-picker'
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

  const renderModelPicker = () => (
    <PlaygroundModelPicker
      values={configuration.selectedModels}
      search={configuration.modelSearchDraft}
      models={configuration.models}
      protocolByModel={configuration.protocolByModel}
      loading={configuration.catalogQuery.isPending}
      error={configuration.catalogInvalid}
      loadingMore={configuration.catalogQuery.isFetchingNextPage}
      hasMore={Boolean(configuration.catalogQuery.hasNextPage)}
      locked={comparison.hasConversation}
      onChange={configuration.setSelectedModels}
      onLoadMore={() => void configuration.catalogQuery.fetchNextPage()}
      onRefresh={() => void configuration.catalogQuery.refetch()}
      onSearch={configuration.setModelSearchDraft}
    />
  )

  const renderSettings = (idPrefix: string) => (
    <PlaygroundSettings
      idPrefix={idPrefix}
      modelPicker={renderModelPicker()}
      apiKeyDraft={configuration.apiKeyDraft}
      apiKeySaved={configuration.apiKey.length > 0}
      apiKeyError={configuration.apiKeyError}
      systemPrompt={configuration.systemPrompt}
      temperatureEnabled={configuration.temperatureEnabled}
      temperature={configuration.temperature}
      maxOutputTokensEnabled={configuration.maxOutputTokensEnabled}
      maxOutputTokens={configuration.maxOutputTokens}
      onApiKeyDraftChange={configuration.updateApiKeyDraft}
      onSaveApiKey={configuration.saveApiKey}
      onClearApiKey={configuration.clearApiKey}
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
    } else if (!configuration.apiKey) status = 'keyRequired'
  }

  const canSend = Boolean(
    configuration.apiKey
    && configuration.selectedModels.length > 0
    && configuration.selectedProtocolsReady
    && !configuration.catalogInvalid
    && comparison.draft.trim()
    && !comparison.streaming
    && !history.saving,
  )

  return (
    <div className="flex min-h-[calc(100dvh-5.75rem)] min-w-0 flex-col gap-3">
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
        modelCount={configuration.selectedModels.length}
        shareControl={(
          <PlaygroundShareControl
            sessions={comparison.sessions}
            streaming={comparison.streaming}
          />
        )}
        status={status}
        onClear={clearConversation}
      />
      <div className="grid min-h-0 min-w-0 flex-1 gap-3 lg:grid-cols-[15rem_minmax(0,1fr)]">
        <aside className="hidden min-h-0 overflow-y-auto rounded-xl border border-[var(--hairline)] bg-surface-1 lg:block">
          {renderSettings('playground-desktop')}
        </aside>
        <PlaygroundComparisonPanel
          apiKeyReady={configuration.apiKey.length > 0}
          canSend={canSend}
          draft={comparison.draft}
          sessions={comparison.sessions}
          status={status}
          streaming={comparison.streaming}
          onDraftChange={comparison.setDraft}
          onSend={comparison.send}
          onStopAll={comparison.stopAll}
          onStopModel={comparison.stopModel}
        />
      </div>
    </div>
  )
}
