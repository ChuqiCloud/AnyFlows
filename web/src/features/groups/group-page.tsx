import { useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useClientTablePagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { useAdminGroupCatalog } from './group-api'
import { GroupDeleteDialog } from './group-delete-dialog'
import { GroupEditorSheet } from './group-editor-sheet'
import { GroupTable } from './group-table'

type EditorState = AdminGroup | 'create' | undefined

/** 提供分组基础倍率、限额与回退策略的结构化管理入口。 */
export function GroupPage() {
  const { t } = useTranslation()
  const groupsQuery = useAdminGroupCatalog()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminGroup>()
  const groups = groupsQuery.data ?? []
  const pagination = useClientTablePagination(groups.length)
  const visibleGroups = groups.slice(pagination.startIndex, pagination.endIndex)
  const exclusiveCount = groups.filter((group) => group.is_exclusive).length
  const peakCount = groups.filter((group) => group.peak_ratio_micros !== null).length

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('groups.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('groups.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={groupsQuery.isFetching} onClick={() => void groupsQuery.refetch()}>
            <RefreshCw className={groupsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('groups.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setEditor('create')}><Plus aria-hidden="true" />{t('groups.actions.create')}</Button>
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('groups.summary.loaded', { count: groups.length })}</span>
        <span>{t('groups.summary.exclusive', { count: exclusiveCount })}</span>
        <span>{t('groups.summary.peak', { count: peakCount })}</span>
      </div>

      {groupsQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('groups.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}</div>
      ) : groupsQuery.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('groups.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('groups.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void groupsQuery.refetch()}>{t('groups.actions.retry')}</Button>
        </div>
      ) : <GroupTable groups={visibleGroups} onDelete={setDeleting} onEdit={setEditor} />}

      {!groupsQuery.isPending && !groupsQuery.isError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={visibleGroups.length}
          hasNextPage={pagination.hasNextPage}
          fetching={groupsQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={pagination.goToNextPage}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <GroupEditorSheet
        open={editor !== undefined}
        group={editor === 'create' ? undefined : editor}
        groups={groups}
        onOpenChange={(open) => !open && setEditor(undefined)}
      />
      <GroupDeleteDialog key={deleting?.id ?? 'no-group'} group={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
