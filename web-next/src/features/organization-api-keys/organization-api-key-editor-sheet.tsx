import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type {
  IssuedOrganizationTokenResponse,
  OrganizationTokenResponse,
} from '@/lib/api/generated/types.gen'
import { OrganizationApiKeyForm } from './organization-api-key-form'

type OrganizationApiKeyEditorSheetProps = {
  organizationId: number
  organizationName: string
  token?: OrganizationTokenResponse
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: IssuedOrganizationTokenResponse) => void
}

export function OrganizationApiKeyEditorSheet(props: OrganizationApiKeyEditorSheetProps) {
  const { t } = useTranslation()
  return (
    <Drawer
      aria-describedby="organization-api-key-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={props.open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={props.onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(props.token ? 'organizationApiKeys.editor.editTitle' : 'organizationApiKeys.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="organization-api-key-editor-description">
                {t(props.token ? 'organizationApiKeys.editor.editDescription' : 'organizationApiKeys.editor.createDescription', { organization: props.organizationName })}
              </p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <OrganizationApiKeyForm
                organizationId={props.organizationId}
                token={props.token}
                onCancel={() => props.onOpenChange(false)}
                onIssued={(issued) => { props.onOpenChange(false); props.onIssued(issued) }}
                onSaved={() => props.onOpenChange(false)}
              />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
