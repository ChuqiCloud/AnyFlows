import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminModel } from '@/lib/api/generated/types.gen'
import { ModelManagementForm } from './model-management-form'

type ModelManagementEditorSheetProps = {
  model?: AdminModel
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ModelManagementEditorSheet({ model, open, onOpenChange }: ModelManagementEditorSheetProps) {
  const { t } = useTranslation()
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="model-management-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(model ? 'modelManagement.editor.editTitle' : 'modelManagement.editor.createTitle')}</SheetTitle>
          <SheetDescription id="model-management-editor-description">
            {t(model ? 'modelManagement.editor.editDescription' : 'modelManagement.editor.createDescription')}
          </SheetDescription>
        </SheetHeader>
        <ModelManagementForm
          mode={model ? 'update' : 'create'}
          model={model}
          onCancel={() => onOpenChange(false)}
          onSaved={() => onOpenChange(false)}
        />
      </SheetContent>
    </Sheet>
  )
}
