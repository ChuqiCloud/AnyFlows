import { Eraser, Settings2 } from 'lucide-react'
import type { ReactNode } from 'react'
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
import { playgroundStatusClassName, type PlaygroundDisplayStatus } from './playground-status'

type PlaygroundPageHeaderProps = {
  clearDisabled: boolean
  historyControl: ReactNode
  mobileSettings: ReactNode
  modeControl: ReactNode
  modelCount: number
  shareControl: ReactNode
  status: PlaygroundDisplayStatus
  onClear: () => void
}

export function PlaygroundPageHeader(props: PlaygroundPageHeaderProps) {
  const { t } = useTranslation()

  return (
    <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="min-w-0">
        <h2 className="text-lg font-semibold">{t('playground.title')}</h2>
        <p className="mt-1 truncate text-xs text-muted-foreground">
          {props.modelCount > 0
            ? t('playground.comparison.summary', { count: props.modelCount })
            : t('playground.model.pending')}
        </p>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-1.5">
        {props.modeControl}
        {props.historyControl}
        {props.shareControl}
        <Badge className={playgroundStatusClassName(props.status)}>
          {t(`playground.status.${props.status}`)}
        </Badge>
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
