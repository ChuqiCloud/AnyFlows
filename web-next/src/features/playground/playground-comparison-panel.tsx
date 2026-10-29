import { Button } from '@heroui/react'
import { ArrowUp, MessageSquareText, PanelRightClose, PanelRightOpen, Square } from 'lucide-react'
import {
  type CSSProperties,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { cn } from '@/lib/utils'
import { PlaygroundSessionColumn } from './playground-session-column'
import type { PlaygroundDisplayStatus } from './playground-status'
import type { PlaygroundSession } from './playground-types'

type PlaygroundComparisonPanelProps = {
  canSend: boolean
  draft: string
  modelPicker: ReactNode
  sessions: PlaygroundSession[]
  settings: ReactNode
  settingsOpen: boolean
  status: PlaygroundDisplayStatus
  streaming: boolean
  onDraftChange: (value: string) => void
  onRetry: (model: string) => Promise<boolean>
  onSend: () => Promise<boolean>
  onSettingsToggle: () => void
  onStopAll: () => void
  onStopModel: (model: string) => void
}

/** 模型替换时沿用首条消息标识，避免同一对话列被 React 重新挂载。 */
function playgroundSessionRenderKey(session: PlaygroundSession) {
  return session.messages[0]?.id ?? `empty:${session.model}`
}

/**
 * 三栏布局：左 = 会话消息流，右 = 参数设置（可折叠），下 = 输入区。
 * 多模型对比时消息流内部仍按列切分，设置栏通过过渡动效不挤压消息区宽度。
 */
export function PlaygroundComparisonPanel(props: PlaygroundComparisonPanelProps) {
  const { t } = useTranslation()
  const [activeModel, setActiveModel] = useState(props.sessions[0]?.model ?? '')
  const composerRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!props.sessions.some((session) => session.model === activeModel)) {
      setActiveModel(props.sessions[0]?.model ?? '')
    }
  }, [activeModel, props.sessions])

  const visibleModel = props.sessions.some((session) => session.model === activeModel)
    ? activeModel
    : props.sessions[0]?.model ?? ''

  const submit = (event?: FormEvent) => {
    event?.preventDefault()
    if (!props.canSend) return
    void props.onSend().finally(() => composerRef.current?.focus())
  }

  // HeroUI Textarea 的 onKeyDown 按 input 元素签名（内部渲染 textarea，但类型统一为 HTMLInputElement）。
  const composerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      submit()
    }
  }

  const columnCount = props.sessions.length
  const trackStyle = {
    '--comparison-columns': Math.max(columnCount, 1),
    '--comparison-column-min': columnCount > 2 ? '22rem' : '0rem',
    '--comparison-track-min': columnCount > 2 ? `${columnCount * 22}rem` : '100%',
  } as CSSProperties

  return (
    <section className="relative grid h-full min-h-[36rem] min-w-0 grid-rows-[minmax(0,1fr)_auto] gap-2.5">
      {/* 上部分：消息流容器 */}
      <div className="relative min-h-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
        {columnCount === 0 ? (
          <div className="ai-workbench-surface grid h-full min-h-0 place-items-center overflow-hidden px-6 py-12 text-center">
            <div className="max-w-sm">
              <div className="mx-auto grid size-11 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 text-info">
                <MessageSquareText className="size-4.5" aria-hidden="true" />
              </div>
              <h3 className="mt-4 text-sm font-semibold">{t('playground.model.pending')}</h3>
              <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
                {t('playground.model.empty')}
              </p>
            </div>
          </div>
        ) : (
          <div className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] xl:grid-rows-1">
            {columnCount > 1 ? (
              <div
                className="flex min-w-0 gap-1 overflow-x-auto border-b border-[var(--hairline)] bg-surface-2/35 p-1 xl:hidden"
                role="tablist"
                aria-label={t('playground.comparison.modelTabs')}
              >
                {props.sessions.map((session) => {
                  const active = session.model === visibleModel
                  return (
                    <button
                      key={session.model}
                      type="button"
                      role="tab"
                      aria-selected={active}
                      className={cn(
                        'min-w-32 flex-1 truncate rounded-lg px-3 py-2 font-mono text-xs text-muted-foreground outline-none transition-[background-color,color,box-shadow] duration-150',
                        'hover:bg-surface-2/70 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60',
                        active && 'bg-surface-1 text-foreground',
                      )}
                      onClick={() => setActiveModel(session.model)}
                    >
                      {session.model}
                    </button>
                  )
                })}
              </div>
            ) : null}
            <div className="min-h-0 overflow-x-auto">
              <div
                className="grid h-full grid-cols-1 xl:min-w-[var(--comparison-track-min)] xl:[grid-template-columns:repeat(var(--comparison-columns),minmax(var(--comparison-column-min),1fr))]"
                style={trackStyle}
              >
                {props.sessions.map((session, index) => (
                  <div
                    key={playgroundSessionRenderKey(session)}
                    className={cn(
                      'min-h-0 min-w-0',
                      index > 0 && 'xl:border-l xl:border-dashed xl:border-[var(--hairline)]',
                      session.model !== visibleModel && 'hidden xl:block',
                    )}
                    role={columnCount > 1 ? 'tabpanel' : undefined}
                  >
                    <PlaygroundSessionColumn
                      session={session}
                      retryDisabled={props.streaming}
                      onRetry={props.onRetry}
                      onStop={props.onStopModel}
                    />
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}

        {/* 桌面端右侧设置栏：浮在消息区之上。 */}
        <aside
          aria-hidden={!props.settingsOpen}
          {...(!props.settingsOpen ? { inert: true } : {})}
          className="playground-settings-panel absolute inset-y-0 right-0 z-10 hidden w-[17.5rem] overflow-y-auto border-l border-[var(--hairline)] bg-surface-1/96 backdrop-blur-xl lg:block"
          data-open={props.settingsOpen}
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
              onClick={props.onSettingsToggle}
            >
              <PanelRightClose className="size-3" aria-hidden="true" />
            </Button>
          </div>
          {props.settings}
        </aside>
      </div>

      {/* 下部分：独立的输入框（v3 PromptInput 范式），宽度与上方消息区一致。 */}
      <form className="w-full" onSubmit={submit}>
        <div className="prompt-composer">
          <div className="prompt-composer__shell">
            <div className="prompt-composer__content">
              <textarea
                ref={composerRef}
                aria-label={t('playground.composer.label')}
                className="prompt-composer__textarea"
                disabled={props.streaming}
                maxLength={100_000}
                placeholder={t('playground.composer.placeholder')}
                rows={3}
                value={props.draft}
                onChange={(event) => {
                  props.onDraftChange(event.target.value)
                  // 自动撑高：先收回再按 scrollHeight 重设，超过 max-height 时 overflow-y 接管滚动。
                  event.target.style.height = 'auto'
                  event.target.style.height = `${event.target.scrollHeight}px`
                }}
                onKeyDown={composerKeyDown}
              />
            </div>

            <div className="prompt-composer__toolbar">
              <div className="prompt-composer__toolbar-start">
                <div className="min-w-0 max-w-[min(100%,18rem)] sm:max-w-[22rem]">
                  {props.modelPicker}
                </div>
                <div className="prompt-composer__status" aria-live="polite">
                  {props.status === 'ready' ? null : props.streaming ? (
                    <AiActivity active label={t(`playground.status.${props.status}`)} size="compact" />
                  ) : (
                    <span>{t(`playground.status.${props.status}`)}</span>
                  )}
                </div>
              </div>

              <div className="prompt-composer__toolbar-end">
                {columnCount > 0 ? (
                  <span className="hidden tabular-nums text-[0.6875rem] text-muted-foreground sm:inline">
                    {t(
                      columnCount === 1
                        ? 'playground.comparison.singleSummary'
                        : 'playground.comparison.summary',
                      { count: columnCount },
                    )}
                  </span>
                ) : null}
                <Button
                  isIconOnly
                  aria-label={t(props.settingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
                  aria-pressed={props.settingsOpen}
                  className="hidden lg:inline-flex"
                  size="sm"
                  title={t(props.settingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
                  type="button"
                  variant="light"
                  onClick={props.onSettingsToggle}
                >
                  {props.settingsOpen ? <PanelRightClose className="size-3.5" aria-hidden="true" /> : <PanelRightOpen className="size-3.5" aria-hidden="true" />}
                </Button>
                {props.streaming ? (
                  <Button
                    isIconOnly
                    aria-label={t('playground.actions.stopAll')}
                    color="danger"
                    size="sm"
                    title={t('playground.actions.stopAll')}
                    type="button"
                    variant="flat"
                    onClick={props.onStopAll}
                  >
                    <Square className="size-3.5" aria-hidden="true" />
                  </Button>
                ) : (
                  <Button
                    isIconOnly
                    aria-label={columnCount > 1 ? t('playground.actions.sendMultiple', { count: columnCount }) : t('playground.actions.send')}
                    color="primary"
                    isDisabled={!props.canSend}
                    radius="full"
                    size="sm"
                    title={columnCount > 1 ? t('playground.actions.sendMultiple', { count: columnCount }) : t('playground.actions.send')}
                    type="submit"
                  >
                    <ArrowUp className="size-4" aria-hidden="true" />
                  </Button>
                )}
              </div>
            </div>
          </div>
        </div>
      </form>
    </section>
  )
}
