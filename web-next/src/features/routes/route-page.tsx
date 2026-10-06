import { useState } from 'react'
import { Button, Skeleton } from '@heroui/react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useClientTablePagination } from '@/components/data-table/use-table-pagination'
import type { AdminRoute } from '@/lib/api/generated/types.gen'
import {
  useAdminRoutes,
  useUpdateAdminRoute,
} from './route-api'
import { RouteDeleteDialog } from './route-delete-dialog'
import { RouteEditorSheet } from './route-editor-sheet'
import { RouteTable } from './route-table'
import { routeToWriteRequest } from './route-form-model'

type EditorState = AdminRoute | 'create' | undefined

/** 管理员智能路由工作区，聚合规则目录、运行统计和候选配置入口。 */
export function RoutePage() {
  const { t } = useTranslation()
  const routesQuery = useAdminRoutes()
  const updateMutation = useUpdateAdminRoute()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminRoute>()
  const routes = routesQuery.data ?? []
  const pagination = useClientTablePagination(routes.length)
  const visibleRoutes = routes.slice(pagination.startIndex, pagination.endIndex)
  const enabledCount = routes.filter((route) => route.enabled).length
  const candidateCount = routes.reduce((sum, route) => sum + route.channels.length, 0)

  const toggle = async (route: AdminRoute) => {
    try {
      await updateMutation.mutateAsync({ id: route.id, body: routeToWriteRequest(route, !route.enabled) })
    } catch {
      // 列表保留原状态，错误由页面统一提示，避免乐观更新掩盖服务端拒绝。
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><h2 className="text-lg font-semibold">{t('routes.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('routes.subtitle')}</p></div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={routesQuery.isFetching} onClick={() => void routesQuery.refetch()}><RefreshCw className={cn('size-3.5', routesQuery.isFetching && 'animate-spin')} aria-hidden="true" />{t('routes.actions.refresh')}</Button>
          <Button type="button" color="primary" size="sm" onClick={() => setEditor('create')}><Plus className="size-3.5" aria-hidden="true" />{t('routes.actions.create')}</Button>
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('routes.summary.loaded', { count: routes.length })}</span>
        <span>{t('routes.summary.enabled', { count: enabledCount })}</span>
        <span>{t('routes.summary.candidates', { count: candidateCount })}</span>
      </div>

      {routesQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('routes.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-xl" />)}</div>
      ) : routesQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4"><h2 className="text-sm font-semibold text-destructive">{t('routes.error.title')}</h2><p className="mt-1 text-xs text-muted-foreground">{t('routes.error.body')}</p><Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void routesQuery.refetch()}>{t('routes.actions.retry')}</Button></div>
      ) : (
        <RouteTable routes={visibleRoutes} pendingToggleId={updateMutation.isPending ? updateMutation.variables?.id : undefined} onEdit={setEditor} onDelete={setDeleting} onToggle={(route) => { void toggle(route) }} />
      )}

      {!routesQuery.isPending && !routesQuery.isError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={visibleRoutes.length}
          hasNextPage={pagination.hasNextPage}
          fetching={routesQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={pagination.goToNextPage}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      {updateMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('routes.errors.updateFailed')}</p> : null}
      <RouteEditorSheet
        open={editor !== undefined}
        route={editor === 'create' ? undefined : editor}
        onOpenChange={(open) => !open && setEditor(undefined)}
        onSaved={() => { setEditor(undefined) }}
      />
      <RouteDeleteDialog key={deleting?.id ?? 'no-route'} route={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
