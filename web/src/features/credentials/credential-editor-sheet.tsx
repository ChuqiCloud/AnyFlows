import { useTranslation } from 'react-i18next'

import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import { useAdminChannelCredentialCatalog } from './credential-api'
import { CredentialForm } from './credential-form'
import { supportsSparkShadow } from './credential-model'

type CredentialEditorSheetProps = {
  open: boolean
  channel?: AdminChannel
  credential?: AdminCredential
  credentials: readonly AdminCredential[]
  onOpenChange: (open: boolean) => void
}

export function CredentialEditorSheet({
  open,
  channel,
  credential,
  credentials,
  onOpenChange,
}: CredentialEditorSheetProps) {
  const { t } = useTranslation()
  const creating = credential === undefined
  const sparkChannel = channel ? supportsSparkShadow(channel) : false
  const catalogQuery = useAdminChannelCredentialCatalog(channel?.id, open && sparkChannel)
  const formCredentials = catalogQuery.data ?? credentials
  const catalogStatus = !sparkChannel || catalogQuery.isSuccess
    ? 'ready'
    : catalogQuery.isError ? 'error' : 'loading'
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl"
        aria-describedby="credential-editor-description"
      >
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(creating ? 'credentials.editor.createTitle' : 'credentials.editor.editTitle')}</SheetTitle>
          <SheetDescription id="credential-editor-description">
            {t(creating ? 'credentials.editor.createDescription' : 'credentials.editor.editDescription', {
              name: channel?.name ?? '',
              id: credential?.id ?? '',
            })}
          </SheetDescription>
        </SheetHeader>
        {channel ? (
          <CredentialForm
            key={`${channel.id}:${credential?.id ?? 'create'}`}
            channel={channel}
            credential={credential}
            credentials={formCredentials}
            credentialCatalogStatus={catalogStatus}
            active={open}
            onSaved={() => onOpenChange(false)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
