import { useState } from 'react'
import { ChevronLeft, ChevronRight, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminUsageLogs } from './usage-log-api'
import { UsageLogTable } from './usage-log-table'

export function UsageLogPage() {
  const { t } = useTranslation()
  const [cursorStack, setCursorStack] = useState<Array<number | undefined>>([undefined])
  const cursor = cursorStack.at(-1)
  const logsQuery = useAdminUsageLogs(cursor)
  const logs = logsQuery.data?.logs ?? []
  const nextCursor = logsQuery.data?.next_cursor

  const openPreviousPage = () => {
    setCursorStack((current) => current.length > 1 ? current.slice(0, -1) : current)
  }

  const openNextPage = () => {
    if (nextCursor !== null && nextCursor !== undefined) {
      setCursorStack((current) => [...current, nextCursor])
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('usageLogs.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('usageLogs.subtitle')}</p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="self-start sm:self-auto"
          disabled={logsQuery.isFetching}
          onClick={() => logsQuery.refetch()}
        >
          <RefreshCw className={logsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('usageLogs.actions.refresh')}
        </Button>
      </header>

      <div className="flex items-center justify-between border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('usageLogs.count', { count: logs.length })}</span>
        <span>{t('usageLogs.page', { page: cursorStack.length })}</span>
      </div>

      {logsQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('usageLogs.loading')}>
          {[0, 1, 2, 3, 4].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
        </div>
      ) : logsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('usageLogs.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('usageLogs.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => logsQuery.refetch()}>
            {t('usageLogs.actions.retry')}
          </Button>
        </div>
      ) : <UsageLogTable logs={logs} />}

      {!logsQuery.isPending
      && !logsQuery.isError
      && (cursorStack.length > 1 || (nextCursor !== null && nextCursor !== undefined)) ? (
        <nav className="flex items-center justify-center gap-2" aria-label={t('usageLogs.pagination')}>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={cursorStack.length === 1 || logsQuery.isFetching}
            onClick={openPreviousPage}
          >
            <ChevronLeft aria-hidden="true" />
            {t('usageLogs.actions.previous')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={nextCursor === null || nextCursor === undefined || logsQuery.isFetching}
            onClick={openNextPage}
          >
            {t('usageLogs.actions.next')}
            <ChevronRight aria-hidden="true" />
          </Button>
        </nav>
      ) : null}
    </div>
  )
}
