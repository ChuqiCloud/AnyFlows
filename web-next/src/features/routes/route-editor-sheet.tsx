import { Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

import type { AdminChannel, AdminRoute } from '@/lib/api/generated/types.gen'
import { useAdminRouteChannelCatalog } from './route-api'
import { RouteForm } from './route-form'

type RouteEditorSheetProps = {
  route?: AdminRoute
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved: (route: AdminRoute) => void
}

const emptyChannels: readonly AdminChannel[] = []

/** 路由编辑使用独立宽面板，保证候选拖拽和运行状态在移动端仍可操作。 */
export function RouteEditorSheet({ route, open, onOpenChange, onSaved }: RouteEditorSheetProps) {
  const { t } = useTranslation()
  const channelsQuery = useAdminRouteChannelCatalog(open)
  const channels = channelsQuery.data ?? emptyChannels
  const mode = route ? 'update' : 'create'
  const descriptionId = 'route-editor-description'
  const retryCandidates = () => { void channelsQuery.refetch() }

  return (
    <Drawer
      aria-describedby={descriptionId}
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-3xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(route ? 'routes.editor.editTitle' : 'routes.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground" id={descriptionId}>{t(route ? 'routes.editor.editDescription' : 'routes.editor.createDescription')}</p>
            </DrawerHeader>
            {/* 表单自带滚动区与底栏，占满剩余高度。 */}
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              {open ? (
                <RouteForm
                  key={`${mode}:${route?.id ?? 'new'}`}
                  mode={mode}
                  route={route}
                  channels={channels}
                  candidatesLoading={channelsQuery.isPending}
                  candidatesError={channelsQuery.isError}
                  onRetryCandidates={retryCandidates}
                  onCancel={() => onOpenChange(false)}
                  onSaved={(savedRoute) => {
                    onOpenChange(false)
                    onSaved(savedRoute)
                  }}
                />
              ) : null}
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
