import { useTranslation } from 'react-i18next'

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { ChannelForm } from './channel-form'
import type { ChannelEditorMode } from './channel-form-model'

type ChannelEditorSheetProps = {
  channel?: AdminChannel
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved?: (channel: AdminChannel, mode: ChannelEditorMode) => void
}

export function ChannelEditorSheet({ channel, open, onOpenChange, onSaved }: ChannelEditorSheetProps) {
  const { t } = useTranslation()
  const mode = channel ? 'update' : 'create'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl"
        aria-describedby="channel-editor-description"
      >
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(channel ? 'channels.editor.editTitle' : 'channels.editor.createTitle')}</SheetTitle>
          <SheetDescription id="channel-editor-description">
            {t(channel ? 'channels.editor.editDescription' : 'channels.editor.createDescription')}
          </SheetDescription>
        </SheetHeader>
        <ChannelForm
          mode={mode}
          channel={channel}
          onCancel={() => onOpenChange(false)}
          onSaved={(savedChannel) => {
            onOpenChange(false)
            onSaved?.(savedChannel, mode)
          }}
        />
      </SheetContent>
    </Sheet>
  )
}
