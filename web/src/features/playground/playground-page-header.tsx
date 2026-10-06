import { Eraser, MessagesSquare, PanelRightOpen, Settings2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
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
import { playgroundStatusClassName, type PlaygroundDisplayStatus } from './playground-status'

type PlaygroundPageHeaderProps = {
  clearDisabled: boolean
  historyControl: ReactNode
  mobileSettings: ReactNode
  modeControl: ReactNode
  modelCount: number
  shareControl: ReactNode
  status: PlaygroundDisplayStatus
  desktopSettingsOpen: boolean
  onClear: () => void
  onDesktopSettingsToggle: () => void
}

export function PlaygroundPageHeader(props: PlaygroundPageHeaderProps) {
  const { t } = useTranslation()
  const active = props.status === 'streaming' || props.status === 'loadingModels'

  return (
    <header className="flex flex-col gap-2.5 rounded-xl border border-[var(--hairline)] bg-surface-1/88 px-3 py-2.5 shadow-subtle backdrop-blur-xl sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-2.5">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-[var(--hairline)] bg-surface-2/70 text-info shadow-subtle">
          <MessagesSquare className="size-4" aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h2 className="truncate text-sm font-semibold">{t('playground.title')}</h2>
          <p className="mt-0.5 truncate text-[0.6875rem] text-muted-foreground">
            {props.modelCount > 0
              ? t(
                  props.modelCount === 1
                    ? 'playground.comparison.singleSummary'
                    : 'playground.comparison.summary',
                  { count: props.modelCount },
                )
              : t('playground.model.pending')}
          </p>
        </div>
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-1.5 sm:justify-end">
        {props.modeControl}
        {props.historyControl}
        {props.shareControl}
        {active ? (
          <AiActivity
            active
            className="hidden h-8 rounded-lg border border-info/15 bg-info/6 px-2.5 sm:flex"
            label={t(`playground.status.${props.status}`)}
            size="compact"
          />
        ) : (
          <Badge className={`hidden sm:inline-flex ${playgroundStatusClassName(props.status)}`}>
            {t(`playground.status.${props.status}`)}
          </Badge>
        )}
        <Sheet>
          <SheetTrigger asChild>
            <Button
              type="button"
              size="icon-sm"
              variant="secondary"
              className="lg:hidden"
              aria-label={t('playground.settings.title')}
            >
              <Settings2 aria-hidden="true" />
            </Button>
          </SheetTrigger>
          <SheetContent className="gap-0 overflow-y-auto sm:max-w-sm">
            <SheetHeader className="border-b border-[var(--hairline)]">
              <SheetTitle>{t('playground.settings.title')}</SheetTitle>
              <SheetDescription className="sr-only">
                {t('playground.settings.description')}
              </SheetDescription>
            </SheetHeader>
            {props.mobileSettings}
          </SheetContent>
        </Sheet>
        <Button
          type="button"
          size="icon-sm"
          variant={props.desktopSettingsOpen ? 'secondary' : 'ghost'}
          className="hidden lg:inline-flex"
          title={t(props.desktopSettingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
          aria-label={t(props.desktopSettingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
          aria-pressed={props.desktopSettingsOpen}
          onClick={props.onDesktopSettingsToggle}
        >
          <PanelRightOpen aria-hidden="true" />
        </Button>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          disabled={props.clearDisabled}
          title={t('playground.actions.clearConversation')}
          aria-label={t('playground.actions.clearConversation')}
          onClick={props.onClear}
        >
          <Eraser aria-hidden="true" />
        </Button>
      </div>
    </header>
  )
}
