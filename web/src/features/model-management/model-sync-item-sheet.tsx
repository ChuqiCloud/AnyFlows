import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'
import type { ModelSyncDraft } from './model-sync-form-model'
import { ModelSyncItemForm } from './model-sync-item-form'

type ModelSyncItemSheetProps = {
  draft?: ModelSyncDraft
  item?: AdminModelSyncPreviewItem
  onClose: () => void
  onSave: (itemId: number, draft: ModelSyncDraft) => void
}

export function ModelSyncItemSheet({ draft, item, onClose, onSave }: ModelSyncItemSheetProps) {
  const { t } = useTranslation()
  return (
    <Sheet open={item !== undefined} onOpenChange={(open) => !open && onClose()}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="model-sync-item-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('modelManagement.sync.editor.title')}</SheetTitle>
          <SheetDescription id="model-sync-item-description">
            {item ? t('modelManagement.sync.editor.description', { model: item.canonical_model }) : ''}
          </SheetDescription>
        </SheetHeader>
        {item && draft ? (
          <ModelSyncItemForm
            draft={draft}
            item={item}
            onCancel={onClose}
            onSave={(values) => onSave(item.item_id, values)}
          />
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
