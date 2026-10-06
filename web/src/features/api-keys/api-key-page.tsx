import { useState } from 'react'
import { Plus, RefreshCw, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useClientTablePagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { IssuedUserToken, UserToken } from '@/lib/api/generated/types.gen'
import { useApiKeys, useUpdateApiKey } from './api-key-api'
import { ApiKeyDeleteDialog } from './api-key-delete-dialog'
import { ApiKeyEditorSheet } from './api-key-editor-sheet'
import { apiKeyRequestWithStatus } from './api-key-form-model'
import { ApiKeyTable, isApiKeyAvailable } from './api-key-table'
import { IssuedApiKeyDialog } from './issued-api-key-dialog'

type EditorState = UserToken | 'create' | undefined

export function ApiKeyPage() {
  const { t } = useTranslation()
  const keysQuery = useApiKeys()
  const toggleMutation = useUpdateApiKey()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<UserToken>()
  const [issued, setIssued] = useState<IssuedUserToken>()
  const [togglingId, setTogglingId] = useState<number>()
  const tokens = keysQuery.data?.tokens ?? []
  const pagination = useClientTablePagination(tokens.length)
  const visibleTokens = tokens.slice(pagination.startIndex, pagination.endIndex)
  const capacity = keysQuery.data?.capacity
  const atCapacity = capacity !== undefined && tokens.length >= capacity
  const activeCount = tokens.filter(isApiKeyAvailable).length

  const toggle = async (token: UserToken) => {
    setTogglingId(token.id)
    try {
      const status = token.status === 'enabled' ? 'disabled' : 'enabled'
      await toggleMutation.mutateAsync({ id: token.id, body: apiKeyRequestWithStatus(token, status) })
    } catch {
      // 服务端数据保持不变，错误提示留在当前列表上下文中。
    } finally {
      setTogglingId(undefined)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('apiKeys.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('apiKeys.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" aria-label={t('apiKeys.actions.refresh')} disabled={keysQuery.isFetching} onClick={() => void keysQuery.refetch()}>
            <RefreshCw className={keysQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('apiKeys.actions.refresh')}
          </Button>
          <Button type="button" size="sm" disabled={keysQuery.isPending || atCapacity} onClick={() => setEditor('create')}>
            <Plus aria-hidden="true" />{t('apiKeys.actions.create')}
          </Button>
        </div>
      </header>

      <div className="flex min-h-8 items-center justify-between gap-3 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('apiKeys.count', { count: tokens.length, active: activeCount })}</span>
        <Badge className={atCapacity ? 'border-warning/25 bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'}>
          {capacity === undefined ? t('apiKeys.capacity.pending') : t('apiKeys.capacity.value', { count: tokens.length, capacity })}
        </Badge>
      </div>

      {atCapacity ? (
        <div className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          <span>{t('apiKeys.capacity.reached')}</span>
        </div>
      ) : null}
      {toggleMutation.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive">
          {t('apiKeys.error.toggle')}
        </div>
      ) : null}

      {keysQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('apiKeys.loading')}>
          {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
        </div>
      ) : keysQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('apiKeys.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('apiKeys.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void keysQuery.refetch()}>{t('apiKeys.actions.retry')}</Button>
        </div>
      ) : (
        <ApiKeyTable tokens={visibleTokens} togglingId={togglingId} onDelete={setDeleting} onEdit={setEditor} onToggle={(token) => void toggle(token)} />
      )}

      {!keysQuery.isPending && !keysQuery.isError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={visibleTokens.length}
          hasNextPage={pagination.hasNextPage}
          fetching={keysQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={pagination.goToNextPage}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <ApiKeyEditorSheet open={editor !== undefined} token={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} onIssued={setIssued} />
      <IssuedApiKeyDialog issued={issued} onClose={() => setIssued(undefined)} />
      <ApiKeyDeleteDialog key={deleting?.id ?? 'no-api-key'} token={deleting} onClose={() => setDeleting(undefined)} onDeleted={() => setDeleting(undefined)} />
    </div>
  )
}
