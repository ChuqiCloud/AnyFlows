import { useTranslation } from 'react-i18next'

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { ChannelCredentialAddForm } from './channel-credential-add-form'
import { ChannelCredentialList } from './channel-credential-list'

type ChannelCredentialsSheetProps = {
  channel?: AdminChannel
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ChannelCredentialsSheet({ channel, open, onOpenChange }: ChannelCredentialsSheetProps) {
  const { t } = useTranslation()
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl"
        aria-describedby="channel-credentials-description"
      >
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('channels.credentials.title')}</SheetTitle>
          <SheetDescription id="channel-credentials-description">
            {t('channels.credentials.description', { name: channel?.name ?? '' })}
          </SheetDescription>
        </SheetHeader>
        <div className="flex-1 overflow-y-auto">
          <ChannelCredentialAddForm channel={channel} open={open} />
          <ChannelCredentialList channel={channel} />
        </div>
      </SheetContent>
    </Sheet>
  )
}
