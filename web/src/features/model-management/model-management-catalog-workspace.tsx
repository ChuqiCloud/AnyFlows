import { Plus, RefreshCw } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminModel } from '@/lib/api/generated/types.gen'
import { useAdminModelMetadata } from './model-management-api'
import { ModelManagementDeleteDialog } from './model-management-delete-dialog'
import { ModelManagementEditorSheet } from './model-management-editor-sheet'
import { ModelManagementTable } from './model-management-table'

type EditorState = AdminModel | 'create' | undefined

/** 管理人工核实后的模型商品元数据及其运营状态。 */
export function ModelManagementCatalogWorkspace() {
  const { t } = useTranslation()
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const modelsQuery = useAdminModelMetadata(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: modelsQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminModel>()
  const models = modelsQuery.data?.pages[pagination.pageIndex]?.models ?? []
  const activeCount = models.filter((model) => model.lifecycle === 'active').length
  const initialError = modelsQuery.isError && modelsQuery.data === undefined

  return (
    <div className="grid gap-4">
      <div className="flex flex-col gap-2 border-y border-[var(--hairline)] py-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
          <span>{t('modelManagement.count', { count: models.length })}</span>
          <span>{t('modelManagement.activeCount', { count: activeCount })}</span>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={modelsQuery.isFetching} onClick={() => void modelsQuery.refetch()}>
            <RefreshCw className={modelsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('modelManagement.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setEditor('create')}>
            <Plus aria-hidden="true" />{t('modelManagement.actions.create')}
          </Button>
        </div>
      </div>

      {modelsQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('modelManagement.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-24 rounded-xl" />)}</div>
      ) : initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('modelManagement.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('modelManagement.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void modelsQuery.refetch()}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : <ModelManagementTable models={models} onDelete={setDeleting} onEdit={setEditor} />}

      {modelsQuery.isFetchNextPageError ? (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('modelManagement.error.more')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void modelsQuery.fetchNextPage()}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {!modelsQuery.isPending && !initialError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={models.length}
          hasNextPage={pagination.hasLoadedNextPage || Boolean(modelsQuery.hasNextPage)}
          fetching={modelsQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => void pagination.goToNextPage(Boolean(modelsQuery.hasNextPage), async () => (await modelsQuery.fetchNextPage()).isSuccess)}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <ModelManagementEditorSheet open={editor !== undefined} model={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} />
      <ModelManagementDeleteDialog key={deleting?.id ?? 'no-model'} model={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
