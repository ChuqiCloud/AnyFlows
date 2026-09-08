import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
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
    <Sheet open={props.open} onOpenChange={props.onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="organization-api-key-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(props.token ? 'organizationApiKeys.editor.editTitle' : 'organizationApiKeys.editor.createTitle')}</SheetTitle>
          <SheetDescription id="organization-api-key-editor-description">
            {t(props.token ? 'organizationApiKeys.editor.editDescription' : 'organizationApiKeys.editor.createDescription', { organization: props.organizationName })}
          </SheetDescription>
        </SheetHeader>
        <OrganizationApiKeyForm
          organizationId={props.organizationId}
          token={props.token}
          onCancel={() => props.onOpenChange(false)}
          onIssued={(issued) => { props.onOpenChange(false); props.onIssued(issued) }}
          onSaved={() => props.onOpenChange(false)}
        />
      </SheetContent>
    </Sheet>
  )
}
