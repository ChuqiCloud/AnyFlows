import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminToken, IssuedAdminToken } from '@/lib/api/generated/types.gen'
import { TokenForm } from './token-form'

type TokenEditorSheetProps = {
  token?: AdminToken
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: IssuedAdminToken) => void
}

export function TokenEditorSheet({ token, open, onOpenChange, onIssued }: TokenEditorSheetProps) {
  const { t } = useTranslation()
  const mode = token ? 'update' : 'create'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="token-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(token ? 'tokens.editor.editTitle' : 'tokens.editor.createTitle')}</SheetTitle>
          <SheetDescription id="token-editor-description">{t(token ? 'tokens.editor.editDescription' : 'tokens.editor.createDescription')}</SheetDescription>
        </SheetHeader>
        <TokenForm
          mode={mode}
          token={token}
          onCancel={() => onOpenChange(false)}
          onIssued={(issued) => { onOpenChange(false); onIssued(issued) }}
          onSaved={() => onOpenChange(false)}
        />
      </SheetContent>
    </Sheet>
  )
}
