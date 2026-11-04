import { useState } from 'react'
import { Button, Skeleton } from '@heroui/react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useUserTokenGroups } from '@/features/api-keys/api-key-group-api'
import { cn } from '@/lib/utils'
import { useUserRoutes } from './user-route-api'
import { UserRouteDeleteDialog } from './user-route-delete-dialog'
import { UserRouteEditor } from './user-route-editor'
import { UserRouteTable } from './user-route-table'
import type { UserRoute } from './user-route-model'

export function UserRoutePage() {
  const { t } = useTranslation()
  const routesQuery = useUserRoutes()
  const groupsQuery = useUserTokenGroups()
  const [editor, setEditor] = useState<UserRoute | 'create'>()
  const [deleting, setDeleting] = useState<UserRoute>()
  const routes = routesQuery.data?.items ?? []
  const capacity = routesQuery.data?.capacity
  const canCreate = routesQuery.isSuccess && (capacity === undefined || routes.length < capacity)

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('userRoutes.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('userRoutes.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={routesQuery.isFetching} onPress={() => void routesQuery.refetch()}>
            <RefreshCw className={cn('size-3.5', routesQuery.isFetching && 'animate-spin')} aria-hidden="true" />{t('userRoutes.actions.refresh')}
          </Button>
          <Button type="button" color="primary" size="sm" isDisabled={!canCreate} onPress={() => setEditor('create')}>
            <Plus className="size-3.5" aria-hidden="true" />{t('userRoutes.actions.create')}
          </Button>
        </div>
      </header>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('userRoutes.summary.loaded', { count: routes.length })}</span>
        {capacity !== undefined ? <span>{t('userRoutes.summary.capacity', { count: routes.length, capacity })}</span> : null}
      </div>
      {groupsQuery.isError ? (
        <div role="alert" className="flex items-center justify-between gap-2 rounded-lg bg-warning/10 p-3 text-xs text-warning">
          <span>{t('userRoutes.groupLoadError')}</span>
          <Button type="button" size="sm" variant="flat" isDisabled={groupsQuery.isFetching} onPress={() => void groupsQuery.refetch()}>{t('userRoutes.actions.retry')}</Button>
        </div>
      ) : null}
      {routesQuery.isPending ? (
        <div className="grid gap-2">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}</div>
      ) : routesQuery.isError ? (
        <div role="alert" className="rounded-lg border border-danger/25 bg-danger/8 p-4">
          <h2 className="text-sm font-semibold text-danger">{t('userRoutes.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('userRoutes.error.body')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onPress={() => void routesQuery.refetch()}>{t('userRoutes.actions.retry')}</Button>
        </div>
      ) : (
        <UserRouteTable routes={routes} groups={groupsQuery.data ?? []} onEdit={setEditor} onDelete={setDeleting} />
      )}
      <UserRouteEditor route={editor === 'create' ? undefined : editor} open={editor !== undefined} onClose={() => setEditor(undefined)} />
      <UserRouteDeleteDialog key={deleting?.id ?? 'no-user-route'} route={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
