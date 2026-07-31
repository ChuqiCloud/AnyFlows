import { useMemo, useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminModel } from '@/lib/api/generated/types.gen'
import { useAdminModelMetadata } from './model-management-api'
import { ModelManagementDeleteDialog } from './model-management-delete-dialog'
import { ModelManagementEditorSheet } from './model-management-editor-sheet'
import { ModelManagementTable } from './model-management-table'

type EditorState = AdminModel | 'create' | undefined

export function ModelManagementPage() {
  const { t } = useTranslation()
  const modelsQuery = useAdminModelMetadata()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminModel>()
  const models = useMemo(() => modelsQuery.data?.pages.flatMap((page) => page.models) ?? [], [modelsQuery.data])
  const activeCount = models.filter((model) => model.lifecycle === 'active').length
  const initialError = modelsQuery.isError && modelsQuery.data === undefined

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('modelManagement.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('modelManagement.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={modelsQuery.isFetching} onClick={() => void modelsQuery.refetch()}>
            <RefreshCw className={modelsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('modelManagement.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setEditor('create')}><Plus aria-hidden="true" />{t('modelManagement.actions.create')}</Button>
        </div>
      </header>

      <div className="flex items-center justify-between border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('modelManagement.count', { count: models.length })}</span>
        <span>{t('modelManagement.activeCount', { count: activeCount })}</span>
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
      {modelsQuery.hasNextPage && !modelsQuery.isFetchNextPageError ? (
        <Button type="button" variant="secondary" className="self-center" disabled={modelsQuery.isFetchingNextPage} onClick={() => void modelsQuery.fetchNextPage()}>
          {t(modelsQuery.isFetchingNextPage ? 'modelManagement.loadingMore' : 'modelManagement.actions.loadMore')}
        </Button>
      ) : null}

      <ModelManagementEditorSheet open={editor !== undefined} model={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} />
      <ModelManagementDeleteDialog key={deleting?.id ?? 'no-model'} model={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
