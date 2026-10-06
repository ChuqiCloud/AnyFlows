import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { Eraser, MessagesSquare, PanelRightOpen, Settings2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
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
  const [mobileSettingsOpen, setMobileSettingsOpen] = useState(false)

  return (
    <header className="flex flex-col gap-2.5 rounded-xl border border-[var(--hairline)] bg-surface-1/88 px-3 py-2.5 backdrop-blur-xl sm:flex-row sm:items-center sm:justify-between">
      <div className="flex min-w-0 items-center gap-2.5">
        <span className="grid size-9 shrink-0 place-items-center rounded-lg border border-[var(--hairline)] bg-surface-2/70 text-info">
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
          <Chip className={`hidden sm:inline-flex ${playgroundStatusClassName(props.status)}`} size="sm" variant="flat">
            {t(`playground.status.${props.status}`)}
          </Chip>
        )}
        {/* HeroUI Drawer 没有 Trigger，用受控开关驱动。 */}
        <Button
          isIconOnly
          aria-label={t('playground.settings.title')}
          className="lg:hidden"
          size="sm"
          type="button"
          variant="bordered"
          onClick={() => setMobileSettingsOpen(true)}
        >
          <Settings2 className="size-3.5" aria-hidden="true" />
        </Button>
        <Drawer
          backdrop="blur"
          classNames={{ base: 'max-h-none' }}
          isOpen={mobileSettingsOpen}
          placement="right"
          scrollBehavior="inside"
          onOpenChange={setMobileSettingsOpen}
        >
          <DrawerContent>
            {() => (
              <>
                <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
                  <h2 className="text-base font-medium text-foreground">{t('playground.settings.title')}</h2>
                  <p className="sr-only">{t('playground.settings.description')}</p>
                </DrawerHeader>
                <DrawerBody className="gap-0 overflow-y-auto p-0">{props.mobileSettings}</DrawerBody>
              </>
            )}
          </DrawerContent>
        </Drawer>
        <Button
          isIconOnly
          aria-label={t(props.desktopSettingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
          aria-pressed={props.desktopSettingsOpen}
          className="hidden lg:inline-flex"
          size="sm"
          title={t(props.desktopSettingsOpen ? 'playground.settings.collapse' : 'playground.settings.expand')}
          type="button"
          variant={props.desktopSettingsOpen ? 'bordered' : 'light'}
          onClick={props.onDesktopSettingsToggle}
        >
          <PanelRightOpen className="size-3.5" aria-hidden="true" />
        </Button>
        <Button
          isIconOnly
          aria-label={t('playground.actions.clearConversation')}
          isDisabled={props.clearDisabled}
          size="sm"
          title={t('playground.actions.clearConversation')}
          type="button"
          variant="light"
          onClick={props.onClear}
        >
          <Eraser className="size-3.5" aria-hidden="true" />
        </Button>
      </div>
    </header>
  )
}
