import { Eraser, MessageSquareText, SendHorizontal, Settings2, Square } from 'lucide-react'
import { type FormEvent, type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
import { Textarea } from '@/components/ui/textarea'
import { useModelCatalog } from '@/features/models/model-api'
import {
  clearPlaygroundApiKey,
  getPlaygroundApiKey,
  isPlaygroundApiKey,
  setPlaygroundApiKey,
} from './playground-api-key'
import { PlaygroundMessageItem } from './playground-message'
import { PlaygroundModelPicker } from './playground-model-picker'
import { PlaygroundSettings } from './playground-settings'
import { usePlaygroundChat } from './use-playground-chat'

export function PlaygroundPage() {
  const { t } = useTranslation()
  const [apiKey, setApiKey] = useState(() => getPlaygroundApiKey() ?? '')
  const [apiKeyDraft, setApiKeyDraft] = useState(apiKey)
  const [apiKeyError, setApiKeyError] = useState<string>()
  const [model, setModel] = useState('')
  const [modelSearchDraft, setModelSearchDraft] = useState('')
  const [modelSearch, setModelSearch] = useState('')
  const [systemPrompt, setSystemPrompt] = useState('')
  const [temperatureEnabled, setTemperatureEnabled] = useState(false)
  const [temperature, setTemperature] = useState(0.7)
  const [maxOutputTokensEnabled, setMaxOutputTokensEnabled] = useState(false)
  const [maxOutputTokens, setMaxOutputTokens] = useState(4096)
  const transcriptRef = useRef<HTMLDivElement>(null)
  const composerRef = useRef<HTMLTextAreaElement>(null)
  const pinnedToBottomRef = useRef(true)

  const catalogQuery = useModelCatalog(modelSearch, 'all', true)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const catalogInvalid = catalogQuery.isError || (pricingScope !== undefined && pricingScope !== 'group')

  useEffect(() => {
    const timer = window.setTimeout(() => setModelSearch(modelSearchDraft.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [modelSearchDraft])

  useEffect(() => {
    if (!model && models[0]) setModel(models[0].model)
  }, [model, models])

  const generationSettings = useMemo(() => ({
    apiKey,
    model,
    systemPrompt,
    temperature: temperatureEnabled ? temperature : undefined,
    maxOutputTokens: maxOutputTokensEnabled ? maxOutputTokens : undefined,
  }), [apiKey, maxOutputTokens, maxOutputTokensEnabled, model, systemPrompt, temperature, temperatureEnabled])
  const chat = usePlaygroundChat(generationSettings)

  useEffect(() => {
    const transcript = transcriptRef.current
    if (transcript && pinnedToBottomRef.current) transcript.scrollTop = transcript.scrollHeight
  }, [chat.messages])

  const saveApiKey = () => {
    if (!isPlaygroundApiKey(apiKeyDraft)) {
      setApiKeyError(t('playground.apiKey.invalid'))
      return
    }
    setPlaygroundApiKey(apiKeyDraft)
    setApiKey(apiKeyDraft.trim())
    setApiKeyDraft(apiKeyDraft.trim())
    setApiKeyError(undefined)
  }

  const clearApiKey = () => {
    clearPlaygroundApiKey()
    setApiKey('')
    setApiKeyDraft('')
    setApiKeyError(undefined)
  }

  const submit = (event?: FormEvent) => {
    event?.preventDefault()
    if (!apiKey || !model || catalogInvalid || chat.requestState === 'streaming' || !chat.draft.trim()) return
    pinnedToBottomRef.current = true
    void chat.send().finally(() => composerRef.current?.focus())
  }

  const composerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      submit()
    }
  }

  const renderModelPicker = () => (
    <PlaygroundModelPicker
      value={model}
      search={modelSearchDraft}
      models={models}
      loading={catalogQuery.isPending}
      error={catalogInvalid}
      loadingMore={catalogQuery.isFetchingNextPage}
      hasMore={catalogQuery.hasNextPage}
      onChange={setModel}
      onLoadMore={() => void catalogQuery.fetchNextPage()}
      onRefresh={() => void catalogQuery.refetch()}
      onSearch={setModelSearchDraft}
    />
  )

  const renderSettings = (idPrefix: string) => (
    <PlaygroundSettings
      idPrefix={idPrefix}
      modelPicker={renderModelPicker()}
      apiKeyDraft={apiKeyDraft}
      apiKeySaved={apiKey.length > 0}
      apiKeyError={apiKeyError}
      systemPrompt={systemPrompt}
      temperatureEnabled={temperatureEnabled}
      temperature={temperature}
      maxOutputTokensEnabled={maxOutputTokensEnabled}
      maxOutputTokens={maxOutputTokens}
      onApiKeyDraftChange={(value) => { setApiKeyDraft(value); setApiKeyError(undefined) }}
      onSaveApiKey={saveApiKey}
      onClearApiKey={clearApiKey}
      onSystemPromptChange={setSystemPrompt}
      onTemperatureEnabledChange={setTemperatureEnabled}
      onTemperatureChange={(value) => setTemperature(Math.min(2, Math.max(0, value)))}
      onMaxOutputTokensEnabledChange={setMaxOutputTokensEnabled}
      onMaxOutputTokensChange={(value) => setMaxOutputTokens(Math.min(1_000_000, Math.max(1, value || 1)))}
    />
  )

  const canSend = Boolean(
    apiKey && model && !catalogInvalid && chat.draft.trim() && chat.requestState !== 'streaming',
  )
  const statusKey = chat.requestState === 'streaming'
    ? 'streaming'
    : chat.requestState === 'error'
      ? 'error'
      : chat.requestState === 'cancelled'
        ? 'cancelled'
        : catalogQuery.isPending
          ? 'loadingModels'
          : catalogInvalid || !model
            ? 'modelUnavailable'
            : !apiKey
              ? 'keyRequired'
              : 'ready'

  const statusClassName = statusKey === 'error' || statusKey === 'modelUnavailable'
    ? 'bg-destructive/12 text-destructive'
    : statusKey === 'cancelled' || statusKey === 'keyRequired'
      ? 'bg-warning/12 text-warning'
      : statusKey === 'streaming' || statusKey === 'loadingModels'
        ? 'bg-info/12 text-info'
        : 'bg-success/12 text-success'

  return (
    <div className="flex min-h-[calc(100dvh-5.75rem)] flex-col gap-3">
      <header className="flex items-end justify-between gap-3">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('playground.title')}</h2>
          <p className="mt-1 truncate text-xs text-muted-foreground">{model || t('playground.model.pending')}</p>
        </div>
        <div className="flex items-center gap-1.5">
          <Badge className={statusClassName}>
            {t(`playground.status.${statusKey}`)}
          </Badge>
          <Sheet>
            <SheetTrigger asChild>
              <Button type="button" size="icon-sm" variant="secondary" className="lg:hidden" aria-label={t('playground.settings.title')}>
                <Settings2 aria-hidden="true" />
              </Button>
            </SheetTrigger>
            <SheetContent className="gap-0 overflow-y-auto sm:max-w-sm">
              <SheetHeader className="border-b border-[var(--hairline)]">
                <SheetTitle>{t('playground.settings.title')}</SheetTitle>
                <SheetDescription className="sr-only">{t('playground.settings.description')}</SheetDescription>
              </SheetHeader>
              {renderSettings('playground-mobile')}
            </SheetContent>
          </Sheet>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            disabled={chat.messages.length === 0 && !chat.draft}
            aria-label={t('playground.actions.clearConversation')}
            onClick={chat.clear}
          >
            <Eraser aria-hidden="true" />
          </Button>
        </div>
      </header>

      <div className="grid min-h-0 flex-1 gap-3 lg:grid-cols-[15rem_minmax(0,1fr)]">
        <aside className="hidden min-h-0 overflow-y-auto rounded-xl border border-[var(--hairline)] bg-surface-1 lg:block">
          {renderSettings('playground-desktop')}
        </aside>

        <section className="grid min-h-[34rem] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
          <div className="flex min-h-9 items-center justify-between gap-3 border-b border-[var(--hairline)] px-3 text-[0.6875rem] text-muted-foreground">
            <span className="truncate font-mono">{model || t('playground.model.pending')}</span>
            {chat.usage ? (
              <span className="shrink-0 tabular-nums">
                {t('playground.usage.summary', {
                  input: chat.usage.inputTokens ?? 0,
                  output: chat.usage.outputTokens ?? 0,
                })}
              </span>
            ) : null}
          </div>

          <div
            ref={transcriptRef}
            className="min-h-0 overflow-y-auto"
            onScroll={(event) => {
              const element = event.currentTarget
              pinnedToBottomRef.current = element.scrollHeight - element.scrollTop - element.clientHeight < 96
            }}
          >
            {chat.messages.length === 0 ? (
              <div className="grid min-h-full place-items-center px-6 py-12 text-center">
                <div className="max-w-sm">
                  <div className="mx-auto grid size-10 place-items-center rounded-xl bg-surface-2 text-muted-foreground">
                    <MessageSquareText className="size-4" aria-hidden="true" />
                  </div>
                  <h3 className="mt-3 text-sm font-semibold">{t('playground.empty.title')}</h3>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('playground.empty.body')}</p>
                </div>
              </div>
            ) : chat.messages.map((message) => <PlaygroundMessageItem key={message.id} message={message} />)}
          </div>

          <form className="border-t border-[var(--hairline)] p-2.5" onSubmit={submit}>
            {chat.errorKind ? (
              <p role="alert" className="mb-2 rounded-lg bg-destructive/8 px-2.5 py-2 text-xs text-destructive">
                {t(`playground.errors.${chat.errorKind}`)}
              </p>
            ) : null}
            <Textarea
              ref={composerRef}
              value={chat.draft}
              className="max-h-40 min-h-20 resize-y border-0 bg-[var(--surface-sunken)] text-sm leading-6 shadow-none focus-visible:ring-2"
              maxLength={100_000}
              placeholder={t(apiKey ? 'playground.composer.placeholder' : 'playground.composer.apiKeyRequired')}
              disabled={chat.requestState === 'streaming'}
              aria-label={t('playground.composer.label')}
              onChange={(event) => chat.setDraft(event.target.value)}
              onKeyDown={composerKeyDown}
            />
            <div className="mt-2 flex min-h-8 items-center justify-between gap-2">
              <span className="text-[0.6875rem] text-muted-foreground" aria-live="polite">
                {t(`playground.status.${statusKey}`)}
              </span>
              {chat.requestState === 'streaming' ? (
                <Button type="button" size="sm" variant="secondary" onClick={chat.stop}>
                  <Square aria-hidden="true" />{t('playground.actions.stop')}
                </Button>
              ) : (
                <Button type="submit" size="sm" disabled={!canSend}>
                  <SendHorizontal aria-hidden="true" />{t('playground.actions.send')}
                </Button>
              )}
            </div>
          </form>
        </section>
      </div>
    </div>
  )
}
