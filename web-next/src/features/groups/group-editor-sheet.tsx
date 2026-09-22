import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="group-editor-description"
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
              <h2 className="text-base font-medium text-foreground">{t(group ? 'groups.editor.editTitle' : 'groups.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="group-editor-description">{t(group ? 'groups.editor.editDescription' : 'groups.editor.createDescription')}</p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <GroupForm mode={mode} group={group} groups={groups} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
