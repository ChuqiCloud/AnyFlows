import { useState } from 'react'
import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import { UserRouteForm } from './user-route-form'
import type { UserRoute } from './user-route-model'

export function UserRouteEditor({ route, open, onClose }: { route?: UserRoute; open: boolean; onClose: () => void }) {
  const { t } = useTranslation()
  const [portalContainer, setPortalContainer] = useState<HTMLDivElement | null>(null)
  return (
    <Drawer aria-describedby="user-route-editor-description" backdrop="blur" classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }} isOpen={open} placement="right" scrollBehavior="inside" onOpenChange={(value) => !value && onClose()}>
      <DrawerContent>{() => (
        <div ref={setPortalContainer} className="relative flex h-full min-h-0 flex-col">
          <DrawerHeader className="flex flex-col gap-1 border-b border-[var(--hairline)] p-4 pr-12">
            <h2 className="text-base font-medium">{t(route ? 'userRoutes.editor.edit' : 'userRoutes.editor.create')}</h2>
            <p id="user-route-editor-description" className="text-xs font-normal leading-5 text-muted-foreground">{t('userRoutes.editor.description')}</p>
          </DrawerHeader>
          <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
            {open ? <UserRouteForm key={route?.id ?? 'create'} route={route} portalContainer={portalContainer} onCancel={onClose} onSaved={onClose} /> : null}
          </DrawerBody>
        </div>
      )}</DrawerContent>
    </Drawer>
  )
}
