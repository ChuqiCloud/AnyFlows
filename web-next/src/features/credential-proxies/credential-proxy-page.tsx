import { useState, type ReactNode } from 'react'
import { Button, Chip, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Skeleton, Switch } from '@heroui/react'
import { Globe2, KeyRound, Network, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination } from '@/components/data-table/data-table-pagination'
import { useClientTablePagination } from '@/components/data-table/use-table-pagination'
import { SiteTooltip } from '@/shared/components/site-tooltip'
import { CredentialProxyEditor } from './credential-proxy-editor'
import {
  useAdminCredentialProxies,
  useDeleteAdminCredentialProxy,
  useUpdateAdminCredentialProxy,
  type AdminCredentialProxy,
} from './credential-proxy-api'

type EditorState = AdminCredentialProxy | 'create' | undefined

/** 管理员专属代理目录，提供结构化出口配置与引用安全状态。 */
export function CredentialProxyPage() {
  const { t } = useTranslation()
  const query = useAdminCredentialProxies()
  const updateMutation = useUpdateAdminCredentialProxy()
  const deleteMutation = useDeleteAdminCredentialProxy()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminCredentialProxy>()
  const proxies = query.data ?? []
  const pagination = useClientTablePagination(proxies.length)
  const visibleProxies = proxies.slice(pagination.startIndex, pagination.endIndex)
  const enabledCount = proxies.filter((proxy) => proxy.enabled).length
  const authenticatedCount = proxies.filter((proxy) => proxy.password_configured).length

  const toggle = async (proxy: AdminCredentialProxy) => {
    try {
      await updateMutation.mutateAsync({ id: proxy.id, body: {
        name: proxy.name, scheme: proxy.scheme, host: proxy.host, port: proxy.port,
        username: proxy.username, password: null, trust_proxy_dns: proxy.trust_proxy_dns,
        enabled: !proxy.enabled,
      } })
    } catch {
      // 服务端会拒绝停用仍被引用的代理，列表保留数据库真实状态。
    }
  }

  const remove = async () => {
    if (!deleting) return
    try {
      await deleteMutation.mutateAsync(deleting.id)
      deleteMutation.reset()
      setDeleting(undefined)
    } catch {
      // 引用冲突使用固定错误态展示，不暴露服务端诊断。
    }
  }

  const openDelete = (proxy: AdminCredentialProxy) => {
    deleteMutation.reset()
    setDeleting(proxy)
  }

  const handleDeleteOpenChange = (open: boolean) => {
    if (open) return
    deleteMutation.reset()
    setDeleting(undefined)
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><h2 className="text-lg font-semibold">{t('credentialProxies.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('credentialProxies.subtitle')}</p></div>
        <div className="flex items-center gap-2"><Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('credentialProxies.actions.refresh')}</Button><Button type="button" size="sm" color="primary" onClick={() => setEditor('create')}><Plus className="size-3.5" aria-hidden="true" />{t('credentialProxies.actions.create')}</Button></div>
      </header>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground"><span>{t('credentialProxies.summary.total', { count: proxies.length })}</span><span>{t('credentialProxies.summary.enabled', { count: enabledCount })}</span><span>{t('credentialProxies.summary.authenticated', { count: authenticatedCount })}</span></div>

      {query.isPending ? <div className="grid gap-2" aria-label={t('credentialProxies.loading')}>{[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-lg" />)}</div>
        : query.isError ? <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4"><h3 className="text-sm font-semibold text-destructive">{t('credentialProxies.errors.title')}</h3><p className="mt-1 text-xs text-muted-foreground">{t('credentialProxies.errors.load')}</p><Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void query.refetch()}>{t('credentialProxies.actions.retry')}</Button></div>
          : proxies.length === 0 ? <div className="grid min-h-44 place-items-center rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center"><div><Network className="mx-auto size-6 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{t('credentialProxies.empty.title')}</h3><p className="mt-1 text-xs text-muted-foreground">{t('credentialProxies.empty.body')}</p></div></div>
            : <ProxyTable proxies={visibleProxies} pendingId={updateMutation.isPending ? updateMutation.variables?.id : undefined} onEdit={setEditor} onDelete={openDelete} onToggle={(proxy) => void toggle(proxy)} />}

      {!query.isPending && !query.isError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={visibleProxies.length}
          hasNextPage={pagination.hasNextPage}
          fetching={query.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={pagination.goToNextPage}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      {updateMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('credentialProxies.errors.toggle')}</p> : null}
      <CredentialProxyEditor open={editor !== undefined} proxy={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} onSaved={() => setEditor(undefined)} />
      <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={deleting !== undefined} onOpenChange={handleDeleteOpenChange}>
        <ModalContent>
          {() => (
            <>
              <ModalHeader className="grid gap-1.5">
                <h2 className="text-base font-semibold">{t('credentialProxies.delete.title')}</h2>
                <p className="text-sm leading-5 font-normal text-muted-foreground">{t('credentialProxies.delete.description', { name: deleting?.name ?? '' })}</p>
              </ModalHeader>
              <ModalBody className="gap-1.5">
                {deleteMutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t('credentialProxies.errors.referenced')}</p> : null}
              </ModalBody>
              <ModalFooter>
                <Button variant="light" isDisabled={deleteMutation.isPending} onPress={() => handleDeleteOpenChange(false)}>{t('credentialProxies.actions.cancel')}</Button>
                <Button color="danger" variant="flat" isDisabled={deleteMutation.isPending} onPress={() => void remove()}>{t('credentialProxies.actions.delete')}</Button>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </div>
  )
}

function ProxyTable({ proxies, pendingId, onEdit, onDelete, onToggle }: { proxies: readonly AdminCredentialProxy[]; pendingId?: number; onEdit: (proxy: AdminCredentialProxy) => void; onDelete: (proxy: AdminCredentialProxy) => void; onToggle: (proxy: AdminCredentialProxy) => void }) {
  const { t } = useTranslation()
  return <div className="overflow-hidden rounded-lg border border-[var(--hairline)]"><div className="hidden grid-cols-[minmax(14rem,1.3fr)_minmax(13rem,1fr)_minmax(12rem,0.9fr)_7rem] border-b border-[var(--hairline)] bg-surface-2/45 px-3 py-2 text-[0.6875rem] font-medium text-muted-foreground md:grid"><span>{t('credentialProxies.columns.proxy')}</span><span>{t('credentialProxies.columns.endpoint')}</span><span>{t('credentialProxies.columns.security')}</span><span className="sr-only">{t('credentialProxies.columns.actions')}</span></div>{proxies.map((proxy) => <div key={proxy.id} className="grid gap-3 border-b border-[var(--hairline)] px-3 py-3 last:border-b-0 md:grid-cols-[minmax(14rem,1.3fr)_minmax(13rem,1fr)_minmax(12rem,0.9fr)_7rem] md:items-center"><div className="min-w-0"><div className="flex items-center gap-2"><span className="truncate text-sm font-semibold">{proxy.name}</span><Chip className={proxy.enabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'} size="sm" variant="flat">{t(proxy.enabled ? 'credentialProxies.status.enabled' : 'credentialProxies.status.disabled')}</Chip></div><p className="mt-1 text-[0.6875rem] text-muted-foreground">#{proxy.id} · v{proxy.version}</p></div><div className="min-w-0"><div className="flex items-center gap-2"><Chip className="bg-info/10 text-info" size="sm" variant="flat">{proxy.scheme.toUpperCase()}</Chip><span className="truncate text-xs">{proxy.host}:{proxy.port}</span></div></div><div className="flex flex-wrap gap-1.5">{proxy.password_configured ? <Chip size="sm" startContent={<KeyRound className="size-3.5" aria-hidden="true" />} variant="flat">{t('credentialProxies.security.auth')}</Chip> : <Chip size="sm" variant="flat">{t('credentialProxies.security.noAuth')}</Chip>}{proxy.trust_proxy_dns ? <Chip className="bg-warning/10 text-warning" size="sm" startContent={<Globe2 className="size-3.5" aria-hidden="true" />} variant="flat">{t('credentialProxies.security.proxyDns')}</Chip> : null}</div><div className="flex items-center justify-end gap-1"><SiteTooltip content={t(proxy.enabled ? 'credentialProxies.actions.disable' : 'credentialProxies.actions.enable')}><span><Switch aria-label={t(proxy.enabled ? 'credentialProxies.actions.disable' : 'credentialProxies.actions.enable')} isDisabled={pendingId === proxy.id} isSelected={proxy.enabled} size="sm" onValueChange={() => onToggle(proxy)} /></span></SiteTooltip><IconButton label={t('credentialProxies.actions.edit')} onClick={() => onEdit(proxy)}><Pencil className="size-3.5" aria-hidden="true" /></IconButton><IconButton label={t('credentialProxies.actions.delete')} destructive onClick={() => onDelete(proxy)}><Trash2 className="size-3.5" aria-hidden="true" /></IconButton></div></div>)}</div>
}

function IconButton({ label, destructive, onClick, children }: { label: string; destructive?: boolean; onClick: () => void; children: ReactNode }) {
  return <SiteTooltip content={label}><Button isIconOnly aria-label={label} className={destructive ? 'text-muted-foreground hover:text-destructive' : undefined} size="sm" type="button" variant="light" onClick={onClick}>{children}</Button></SiteTooltip>
}
