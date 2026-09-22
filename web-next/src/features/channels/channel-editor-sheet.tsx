import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="channel-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(channel ? 'channels.editor.editTitle' : 'channels.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="channel-editor-description">
                {t(channel ? 'channels.editor.editDescription' : 'channels.editor.createDescription')}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <ChannelForm
                mode={mode}
                channel={channel}
                onCancel={() => onOpenChange(false)}
                onSaved={(savedChannel) => {
                  onOpenChange(false)
                  onSaved?.(savedChannel, mode)
                }}
              />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
