import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { IssuedUserToken, UserToken } from '@/lib/api/generated/types.gen'
import { ApiKeyForm } from './api-key-form'

type ApiKeyEditorSheetProps = {
  token?: UserToken
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: IssuedUserToken) => void
}

export function ApiKeyEditorSheet(props: ApiKeyEditorSheetProps) {
  const { t } = useTranslation()
  return (
    <Sheet open={props.open} onOpenChange={props.onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="api-key-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(props.token ? 'apiKeys.editor.editTitle' : 'apiKeys.editor.createTitle')}</SheetTitle>
          <SheetDescription id="api-key-editor-description">
            {t(props.token ? 'apiKeys.editor.editDescription' : 'apiKeys.editor.createDescription')}
          </SheetDescription>
        </SheetHeader>
        <ApiKeyForm
          token={props.token}
          onCancel={() => props.onOpenChange(false)}
          onIssued={(issued) => { props.onOpenChange(false); props.onIssued(issued) }}
          onSaved={() => props.onOpenChange(false)}
        />
      </SheetContent>
    </Sheet>
  )
}
