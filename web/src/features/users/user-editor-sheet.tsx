import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { UserForm } from './user-form'

type UserEditorSheetProps = {
  user?: AdminUser
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function UserEditorSheet({ user, open, onOpenChange }: UserEditorSheetProps) {
  const { t } = useTranslation()
  const mode = user ? 'update' : 'create'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="user-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(user ? 'users.editor.editTitle' : 'users.editor.createTitle')}</SheetTitle>
          <SheetDescription id="user-editor-description">{t(user ? 'users.editor.editDescription' : 'users.editor.createDescription')}</SheetDescription>
        </SheetHeader>
        <UserForm mode={mode} user={user} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
      </SheetContent>
    </Sheet>
  )
}
