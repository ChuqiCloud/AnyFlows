import { useState } from 'react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { useAdminChannels, useProbeAdminChannel } from './channel-api'
import { ChannelEditorSheet } from './channel-editor-sheet'
import { ChannelTable, type ProbeDisplay } from './channel-table'

type EditorState = AdminChannel | 'create' | undefined

export function ChannelPage() {
  const { t } = useTranslation()
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const channelsQuery = useAdminChannels(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: channelsQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const probeMutation = useProbeAdminChannel()
  const [editor, setEditor] = useState<EditorState>()
  const [probingId, setProbingId] = useState<number>()
  const [probeResults, setProbeResults] = useState<Record<number, ProbeDisplay>>({})
  const channels = channelsQuery.data?.pages[pagination.pageIndex]?.channels ?? []
  const initialError = channelsQuery.isError && channelsQuery.data === undefined

  const probe = async (channelId: number) => {
    setProbingId(channelId)
    try {
      const result = await probeMutation.mutateAsync(channelId)
      setProbeResults((current) => ({ ...current, [channelId]: result }))
    } catch {
      setProbeResults((current) => ({ ...current, [channelId]: { failed: true } }))
    } finally {
      setProbingId(undefined)
    }
  }

  const openCredentials = (channel: AdminChannel) => {
    window.location.hash = `#/console/credentials?channel=${channel.id}`
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('channels.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('channels.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="secondary" aria-label={t('channels.actions.refresh')} disabled={channelsQuery.isFetching} onClick={() => channelsQuery.refetch()}>
            <RefreshCw className={channelsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('channels.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setEditor('create')}><Plus aria-hidden="true" />{t('channels.actions.create')}</Button>
        </div>
      </header>

      <div className="flex items-center justify-between border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('channels.count', { count: channels.length })}</span>
        <span>{t('channels.scope')}</span>
      </div>

      {channelsQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('channels.loading')}>
          {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
        </div>
      ) : initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('channels.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('channels.error.body')}</p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => channelsQuery.refetch()}>{t('channels.actions.retry')}</Button>
        </div>
      ) : (
        <ChannelTable channels={channels} probeResults={probeResults} probingId={probingId} onCredentials={openCredentials} onEdit={setEditor} onProbe={probe} />
      )}

      {channelsQuery.isFetchNextPageError ? <p role="alert" className="text-xs text-destructive">{t('channels.error.body')}</p> : null}
      {!channelsQuery.isPending && !initialError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={channels.length}
          hasNextPage={pagination.hasLoadedNextPage || Boolean(channelsQuery.hasNextPage)}
          fetching={channelsQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => void pagination.goToNextPage(Boolean(channelsQuery.hasNextPage), async () => (await channelsQuery.fetchNextPage()).isSuccess)}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <ChannelEditorSheet
        open={editor !== undefined}
        channel={editor === 'create' ? undefined : editor}
        onOpenChange={(open) => !open && setEditor(undefined)}
        onSaved={(channel, mode) => {
          if (mode === 'create') openCredentials(channel)
        }}
      />
    </div>
  )
}
