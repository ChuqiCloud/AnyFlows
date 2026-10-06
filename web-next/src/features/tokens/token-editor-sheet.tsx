import { useTranslation } from 'react-i18next'
import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'

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
    <Drawer
      aria-describedby="token-editor-description"
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
              <h2 className="text-base font-medium text-foreground">{t(token ? 'tokens.editor.editTitle' : 'tokens.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="token-editor-description">{t(token ? 'tokens.editor.editDescription' : 'tokens.editor.createDescription')}</p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <TokenForm
                mode={mode}
                token={token}
                onCancel={() => onOpenChange(false)}
                onIssued={(issued) => { onOpenChange(false); onIssued(issued) }}
                onSaved={() => onOpenChange(false)}
              />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
