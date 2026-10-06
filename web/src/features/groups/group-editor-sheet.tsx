import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { GroupForm } from './group-form'

type GroupEditorSheetProps = {
  group?: AdminGroup
  groups: AdminGroup[]
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function GroupEditorSheet({ group, groups, open, onOpenChange }: GroupEditorSheetProps) {
  const { t } = useTranslation()
  const mode = group ? 'update' : 'create'

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="group-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(group ? 'groups.editor.editTitle' : 'groups.editor.createTitle')}</SheetTitle>
          <SheetDescription id="group-editor-description">{t(group ? 'groups.editor.editDescription' : 'groups.editor.createDescription')}</SheetDescription>
        </SheetHeader>
        <GroupForm mode={mode} group={group} groups={groups} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
      </SheetContent>
    </Sheet>
  )
}
