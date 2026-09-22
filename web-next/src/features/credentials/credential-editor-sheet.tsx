import { useTranslation } from 'react-i18next'

import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
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
    <Drawer
      aria-describedby="credential-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(creating ? 'credentials.editor.createTitle' : 'credentials.editor.editTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="credential-editor-description">
                {t(creating ? 'credentials.editor.createDescription' : 'credentials.editor.editDescription', {
                  name: channel?.name ?? '',
                  id: credential?.id ?? '',
                })}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
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
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
