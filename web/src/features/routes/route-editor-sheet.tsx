import { useTranslation } from 'react-i18next'

import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
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
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-3xl" aria-describedby={descriptionId}>
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(route ? 'routes.editor.editTitle' : 'routes.editor.createTitle')}</SheetTitle>
          <SheetDescription id={descriptionId}>{t(route ? 'routes.editor.editDescription' : 'routes.editor.createDescription')}</SheetDescription>
        </SheetHeader>
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
      </SheetContent>
    </Sheet>
  )
}
