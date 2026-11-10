import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { RefreshCw, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { capabilityError, useCredentialModels, useRefreshCredentialModels } from './key-capability-api'
import { CapabilityError, CapabilityTime } from './key-capability-fields'
import { UpstreamModelPolicyEditor } from './upstream-model-policy-editor'

export function CredentialModelDrawer({ channelId, credentialId, fetchError, onClose }: { channelId: number; credentialId: number; fetchError?: string; onClose: () => void }) {
  const { t } = useTranslation()
  const query = useCredentialModels(channelId, credentialId)
  const refresh = useRefreshCredentialModels(channelId, credentialId)
  const fetchModels = async () => { try { await refresh.mutateAsync() } catch { /* Keep previous observations visible. */ } }
  return <Drawer isOpen size="3xl" onOpenChange={(open) => { if (!open) onClose() }}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader className="flex flex-wrap items-center justify-between gap-3 pr-12"><h2 className="text-base font-semibold">{t('keyCapabilities.modelsTitle', { id: credentialId })}</h2><div className="flex items-center gap-2"><Button type="button" variant="bordered" size="sm" isLoading={refresh.isPending} onPress={() => void fetchModels()}><RefreshCw className="size-3.5" />{t('keyCapabilities.fetchModels')}</Button><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('keyCapabilities.close')} onPress={onClose}><X className="size-4" /></Button></div></DrawerHeader><DrawerBody className="gap-4 pb-6">
    {query.isPending ? <p role="status" className="text-xs">{t('keyCapabilities.loading')}</p> : query.isError ? <CapabilityError onRetry={() => void query.refetch()} /> : query.data ? <><div className="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-muted-foreground"><span>{t(`keyCapabilities.discoveryStatus.${query.data.status}`, { defaultValue: t('keyCapabilities.unknown') })}</span><span>{t('keyCapabilities.lastSuccess')}: <CapabilityTime value={query.data.last_success_at} /></span><span>{t('keyCapabilities.lastChecked')}: <CapabilityTime value={query.data.last_checked_at} /></span></div>{query.data.status === 'unavailable' ? <CapabilityError code="discoveryFailed" onRetry={() => void fetchModels()} /> : null}{query.data.status === 'empty' ? <p role="status" className="text-xs text-warning">{t('keyCapabilities.emptyDiscovery')}</p> : null}<UpstreamModelPolicyEditor channelId={channelId} credentialId={credentialId} observation={query.data} /></> : null}
    {refresh.isError ? <CapabilityError code={capabilityError(refresh.error)} onRetry={() => void fetchModels()} /> : null}
    {fetchError && !refresh.isSuccess && !refresh.isError ? <CapabilityError code={fetchError} onRetry={() => void fetchModels()} /> : null}
  </DrawerBody></DrawerContent></Drawer>
}
