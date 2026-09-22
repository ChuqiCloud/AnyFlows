import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="user-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(user ? 'users.editor.editTitle' : 'users.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="user-editor-description">{t(user ? 'users.editor.editDescription' : 'users.editor.createDescription')}</p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <UserForm mode={mode} user={user} onCancel={() => onOpenChange(false)} onSaved={() => onOpenChange(false)} />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
