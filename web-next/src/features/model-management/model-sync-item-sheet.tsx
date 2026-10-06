import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="model-sync-item-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={item !== undefined}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={(open) => !open && onClose()}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('modelManagement.sync.editor.title')}</h2>
              <p className="text-sm text-muted-foreground" id="model-sync-item-description">
                {item ? t('modelManagement.sync.editor.description', { model: item.canonical_model }) : ''}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              {item && draft ? (
                <ModelSyncItemForm
                  draft={draft}
                  item={item}
                  onCancel={onClose}
                  onSave={(values) => onSave(item.item_id, values)}
                />
              ) : null}
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
