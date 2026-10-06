import { useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminToken, IssuedAdminToken } from '@/lib/api/generated/types.gen'
import { IssuedTokenDialog } from './issued-token-dialog'
import { TokenDeleteDialog } from './token-delete-dialog'
import { TokenEditorSheet } from './token-editor-sheet'
import { useAdminTokens } from './token-api'
import { TokenTable } from './token-table'

type EditorState = AdminToken | 'create' | undefined

export function TokenPage() {
  const { t } = useTranslation()
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const tokensQuery = useAdminTokens(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: tokensQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminToken>()
  const [issued, setIssued] = useState<IssuedAdminToken>()
  const tokens = tokensQuery.data?.pages[pagination.pageIndex]?.tokens ?? []
  const activeCount = tokens.filter((token) => token.status === 'enabled' && (token.expired_at === null || token.expired_at > Date.now() / 1000)).length
  const initialError = tokensQuery.isError && tokensQuery.data === undefined

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('tokens.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('tokens.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" aria-label={t('tokens.actions.refresh')} disabled={tokensQuery.isFetching} onClick={() => tokensQuery.refetch()}>
            <RefreshCw className={tokensQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('tokens.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setEditor('create')}><Plus aria-hidden="true" />{t('tokens.actions.create')}</Button>
        </div>
      </header>

      <div className="flex items-center justify-between border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('tokens.count', { count: tokens.length })}</span>
        <span>{t('tokens.activeCount', { count: activeCount })}</span>
      </div>

      {tokensQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('tokens.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}</div>
      ) : initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('tokens.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('tokens.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => tokensQuery.refetch()}>{t('tokens.actions.retry')}</Button>
        </div>
      ) : <TokenTable tokens={tokens} onDelete={setDeleting} onEdit={setEditor} />}

      {tokensQuery.isFetchNextPageError ? <p role="alert" className="text-xs text-destructive">{t('tokens.error.body')}</p> : null}
      {!tokensQuery.isPending && !initialError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={tokens.length}
          hasNextPage={pagination.hasLoadedNextPage || Boolean(tokensQuery.hasNextPage)}
          fetching={tokensQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => void pagination.goToNextPage(Boolean(tokensQuery.hasNextPage), async () => (await tokensQuery.fetchNextPage()).isSuccess)}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <TokenEditorSheet open={editor !== undefined} token={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} onIssued={setIssued} />
      <IssuedTokenDialog issued={issued} onClose={() => setIssued(undefined)} />
      <TokenDeleteDialog key={deleting?.id ?? 'no-token'} token={deleting} onClose={() => setDeleting(undefined)} onDeleted={() => setDeleting(undefined)} />
    </div>
  )
}
