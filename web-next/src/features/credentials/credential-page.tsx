import { useCallback, useEffect, useMemo, useState } from 'react'
import { Boxes, Download, FileUp, KeyRound, Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button, Modal, ModalBody, ModalContent, ModalHeader, Skeleton } from '@heroui/react'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { navigateTo } from '@/lib/router-navigation'
import { useAdminChannel, useAdminChannels } from '@/features/channels/channel-api'
import { exportAdminCredentialFiles, useAdminChannelCredentials, type AdminCredentialImportResult } from './credential-api'
import { CredentialChannelToolbar } from './credential-channel-toolbar'
import { CredentialEditorSheet } from './credential-editor-sheet'
import { CredentialImport } from './credential-import'
import { credentialKindsForChannel } from './credential-model'
import { CredentialList } from './credential-list'
import { CredentialSummary } from './credential-summary'

type EditorState = AdminCredential | 'create' | undefined

/** 管理员独立账号池工作区；渠道始终是凭据 API 的所有权边界。 */
export function CredentialPage({ initialChannelId }: { initialChannelId?: number }) {
  const { t } = useTranslation()
  const channelsQuery = useAdminChannels()
  const channels = useMemo(() => channelsQuery.data?.pages.flatMap((page) => page.channels) ?? [], [channelsQuery.data])
  const [selectedId, setSelectedId] = useState(initialChannelId)
  const listedChannel = channels.find((channel) => channel.id === selectedId)
  const detailQuery = useAdminChannel(selectedId, selectedId !== undefined && listedChannel === undefined)
  const selectedChannel = listedChannel ?? detailQuery.data ?? (selectedId === undefined ? channels[0] : undefined)
  const visibleChannels = useMemo(() => selectedChannel && !channels.some((channel) => channel.id === selectedChannel.id)
    ? [selectedChannel, ...channels]
    : channels, [channels, selectedChannel])
  const credentialsQuery = useAdminChannelCredentials(selectedChannel?.id)
  const { fetchNextPage: fetchNextCredentialsPage, refetch: refetchCredentials } = credentialsQuery
  const credentials = useMemo(() => credentialsQuery.data?.pages.flatMap((page) => page.credentials) ?? [], [credentialsQuery.data])
  const [editor, setEditor] = useState<EditorState>()
  const [exporting, setExporting] = useState(false)
  const [importResult, setImportResult] = useState<AdminCredentialImportResult>()
  const [importOpen, setImportOpen] = useState(false)
  const [fileError, setFileError] = useState<string>()

  const selectChannel = useCallback((channelId: number) => {
    setSelectedId(channelId)
    navigateTo(`/console/credentials?channel=${channelId}`)
  }, [])

  useEffect(() => setSelectedId(initialChannelId), [initialChannelId])
  useEffect(() => {
    if (selectedId !== undefined || channels.length === 0) return
    selectChannel(channels[0].id)
  }, [channels, selectChannel, selectedId])
  useEffect(() => {
    if (!detailQuery.isError || channels.length === 0) return
    selectChannel(channels[0].id)
  }, [channels, detailQuery.isError, selectChannel])
  useEffect(() => {
    setEditor(undefined)
    setImportResult(undefined)
    setFileError(undefined)
  }, [selectedChannel?.id])
  const refreshCredentials = useCallback(async () => { await refetchCredentials() }, [refetchCredentials])
  const loadMoreCredentials = useCallback(async () => { await fetchNextCredentialsPage() }, [fetchNextCredentialsPage])
  const refreshAll = async () => {
    await Promise.all([channelsQuery.refetch(), selectedChannel ? credentialsQuery.refetch() : Promise.resolve()])
  }
  const exportFiles = async () => {
    if (!selectedChannel || selectedChannel.type !== 'openai') return
    setFileError(undefined)
    setExporting(true)
    try {
      const blob = await exportAdminCredentialFiles(selectedChannel.id)
      const url = URL.createObjectURL(blob); const anchor = document.createElement('a')
      anchor.href = url; anchor.download = 'anyflows-codex-credentials.json'; anchor.click()
      window.setTimeout(() => URL.revokeObjectURL(url), 1000)
    } catch { setFileError(t('credentials.fileTransfer.exportFailed')) } finally { setExporting(false) }
  }

  const channelPending = channelsQuery.isPending || (selectedId !== undefined && !listedChannel && detailQuery.isPending)
  const channelError = channelsQuery.isError || (selectedId !== undefined && !listedChannel && detailQuery.isError && channels.length === 0)
  const importKinds = selectedChannel ? credentialKindsForChannel(selectedChannel.type, selectedChannel.provider) : []
  const canImportCredentials = importKinds.includes('api_key') || importKinds.includes('oauth')
  const canImportCodex = selectedChannel?.type === 'openai'

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><h2 className="text-lg font-semibold">{t('credentials.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('credentials.subtitle')}</p></div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={channelsQuery.isFetching || credentialsQuery.isFetching} onClick={refreshAll}><RefreshCw className={`size-3.5 ${channelsQuery.isFetching || credentialsQuery.isFetching ? 'animate-spin' : ''}`} aria-hidden="true" />{t('credentials.actions.refresh')}</Button>
          <Button type="button" size="sm" variant="bordered" isDisabled={!selectedChannel} onClick={() => selectedChannel && navigateTo(`/console/channel-models?channel=${selectedChannel.id}`)}><Boxes className="size-3.5" aria-hidden="true" />{t('credentials.actions.models')}</Button>
          <Button type="button" color="primary" size="sm" isDisabled={!selectedChannel} onClick={() => setEditor('create')}><Plus className="size-3.5" aria-hidden="true" />{t('credentials.actions.add')}</Button>
          {canImportCredentials ? <Button type="button" size="sm" variant="flat" onClick={() => setImportOpen(true)}><FileUp className="size-3.5" aria-hidden="true" />{t('credentials.batch.importTitle')}</Button> : null}
          {canImportCodex ? <Button type="button" size="sm" variant="flat" isDisabled={exporting} onClick={() => void exportFiles()}><Download className="size-3.5" aria-hidden="true" />{t(exporting ? 'credentials.fileTransfer.exporting' : 'credentials.fileTransfer.export')}</Button> : null}
        </div>
      </header>

      {fileError ? <p role="alert" className="text-sm text-danger">{fileError}</p> : null}
      {importResult ? <div role="status" className="border-l-2 border-primary pl-3 text-sm">
        <p>{t('credentials.fileTransfer.result', importResult)}</p>
        {importResult.failed > 0 ? <details className="mt-1 text-default-500"><summary className="cursor-pointer">{t('credentials.fileTransfer.failedItems')}</summary><ul className="mt-2 max-h-40 overflow-auto text-xs">{importResult.items.filter((item) => item.action === 'failed').map((item, index) => <li key={`${item.file}-${item.index}-${index}`}>{t('credentials.fileTransfer.failedItem', { file: item.file, index: item.index + 1, message: item.message })}</li>)}</ul></details> : null}
      </div> : null}

      {channelPending ? (
        <div className="grid gap-3"><Skeleton className="h-16 rounded-xl" /><Skeleton className="h-56 rounded-xl" /></div>
      ) : channelError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4"><h3 className="text-sm font-semibold text-destructive">{t('credentials.channel.loadFailed')}</h3><p className="mt-1 text-xs text-muted-foreground">{t('credentials.channel.loadFailedDescription')}</p><Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => channelsQuery.refetch()}>{t('credentials.actions.retry')}</Button></div>
      ) : visibleChannels.length === 0 ? (
        <div className="grid min-h-64 place-items-center rounded-xl border border-[var(--hairline)] text-center"><div><KeyRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{t('credentials.channel.empty')}</h3><p className="mt-1 text-xs text-muted-foreground">{t('credentials.channel.emptyDescription')}</p><Button as="a" color="primary" href="/console/channels" size="sm" className="mt-4">{t('credentials.channel.openChannels')}</Button></div></div>
      ) : selectedChannel ? (
        <>
          <CredentialChannelToolbar channels={visibleChannels} selected={selectedChannel} loadingMore={channelsQuery.isFetchingNextPage} hasNextPage={Boolean(channelsQuery.hasNextPage)} onSelect={selectChannel} onLoadMore={() => { void channelsQuery.fetchNextPage() }} />
          <CredentialSummary credentials={credentials} />
          <CredentialList key={selectedChannel.id} channel={selectedChannel} credentials={credentials} loading={credentialsQuery.isPending} loadError={credentialsQuery.isError && !credentialsQuery.data} refreshing={credentialsQuery.isFetching} refreshError={credentialsQuery.isRefetchError} loadingMore={credentialsQuery.isFetchingNextPage} hasNextPage={Boolean(credentialsQuery.hasNextPage)} active onRefresh={refreshCredentials} onLoadMore={loadMoreCredentials} onEdit={setEditor} />
          <CredentialEditorSheet key={`${selectedChannel.id}:${editor === 'create' ? 'create' : editor?.id ?? 'closed'}`} open={editor !== undefined} channel={selectedChannel} credential={editor === 'create' ? undefined : editor} credentials={credentials} onOpenChange={(open) => { if (!open) setEditor(undefined) }} />
          <Modal isOpen={importOpen} onOpenChange={setImportOpen} placement="center" size="md">
            <ModalContent>{(onClose) => <><ModalHeader className="flex flex-col gap-1"><span>{t('credentials.batch.importTitle')}</span><span className="text-sm font-normal text-muted-foreground">{t('credentials.batch.importHint', { defaultValue: '粘贴 Key 或选择文件，导入到当前渠道。' })}</span></ModalHeader><ModalBody>{selectedChannel ? <CredentialImport key={`${selectedChannel.id}-modal`} channel={selectedChannel} onImported={async (result) => { setImportResult(result); setImportOpen(false); onClose(); await refetchCredentials() }} /> : null}</ModalBody></>}</ModalContent>
          </Modal>
        </>
      ) : null}
    </div>
  )
}
