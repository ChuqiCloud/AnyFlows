import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="model-management-editor-description"
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
              <h2 className="text-base font-medium text-foreground">{t(model ? 'modelManagement.editor.editTitle' : 'modelManagement.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="model-management-editor-description">
                {t(model ? 'modelManagement.editor.editDescription' : 'modelManagement.editor.createDescription')}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <ModelManagementForm
                mode={model ? 'update' : 'create'}
                model={model}
                onCancel={() => onOpenChange(false)}
                onSaved={() => onOpenChange(false)}
              />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
