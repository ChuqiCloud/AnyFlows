import { useMemo, useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
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
  const tokensQuery = useAdminTokens()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminToken>()
  const [issued, setIssued] = useState<IssuedAdminToken>()
  const tokens = useMemo(() => tokensQuery.data?.pages.flatMap((page) => page.tokens) ?? [], [tokensQuery.data])
  const activeCount = tokens.filter((token) => token.status === 'enabled' && (token.expired_at === null || token.expired_at > Date.now() / 1000)).length

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
      ) : tokensQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('tokens.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('tokens.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => tokensQuery.refetch()}>{t('tokens.actions.retry')}</Button>
        </div>
      ) : <TokenTable tokens={tokens} onDelete={setDeleting} onEdit={setEditor} />}

      {tokensQuery.hasNextPage ? (
        <Button type="button" variant="secondary" className="self-center" disabled={tokensQuery.isFetchingNextPage} onClick={() => tokensQuery.fetchNextPage()}>
          {t(tokensQuery.isFetchingNextPage ? 'tokens.loadingMore' : 'tokens.actions.loadMore')}
        </Button>
      ) : null}

      <TokenEditorSheet open={editor !== undefined} token={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} onIssued={setIssued} />
      <IssuedTokenDialog issued={issued} onClose={() => setIssued(undefined)} />
      <TokenDeleteDialog key={deleting?.id ?? 'no-token'} token={deleting} onClose={() => setDeleting(undefined)} onDeleted={() => setDeleting(undefined)} />
    </div>
  )
}
