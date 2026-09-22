import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type { IssuedUserToken, UserToken } from '@/lib/api/generated/types.gen'
import { ApiKeyForm } from './api-key-form'

type ApiKeyEditorSheetProps = {
  token?: UserToken
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: IssuedUserToken) => void
}

export function ApiKeyEditorSheet(props: ApiKeyEditorSheetProps) {
  const { t } = useTranslation()
  return (
    <Drawer
      aria-describedby="api-key-editor-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={props.open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={props.onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(props.token ? 'apiKeys.editor.editTitle' : 'apiKeys.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id="api-key-editor-description">
                {t(props.token ? 'apiKeys.editor.editDescription' : 'apiKeys.editor.createDescription')}
              </p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <ApiKeyForm
                token={props.token}
                onCancel={() => props.onOpenChange(false)}
                onIssued={(issued) => { props.onOpenChange(false); props.onIssued(issued) }}
                onSaved={() => props.onOpenChange(false)}
              />
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
