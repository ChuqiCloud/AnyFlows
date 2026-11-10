import { useState } from 'react'
import { Button } from '@heroui/react'
import { Boxes, Gauge, ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { SiteTooltip } from '@/shared/components/site-tooltip'
import { capabilityError, useCredentialModels, useRefreshCredentialModels } from './key-capability-api'
import { CredentialModelDrawer } from './credential-model-drawer'
import { CredentialUsageDrawer } from './credential-usage-drawer'

export function CredentialCapabilityActions({ channelId, credentialId, active, disabled }: { channelId: number; credentialId: number; active: boolean; disabled: boolean }) {
  const { t } = useTranslation()
  const [panel, setPanel] = useState<'models' | 'usage'>()
  const models = useCredentialModels(channelId, credentialId, false)
  const refresh = useRefreshCredentialModels(channelId, credentialId)
  const fetchModels = async () => { try { await refresh.mutateAsync() } catch { /* The drawer retains the target and a retry action. */ } finally { setPanel('models') } }
  return <>
    {models.data ? <span className="mr-1 text-[11px] text-muted-foreground tabular-nums">{t('keyCapabilities.modelCount', { count: models.data.items.filter((item) => item.enabled).length })}</span> : null}
    <SiteTooltip content={t('keyCapabilities.fetchModels')}><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('keyCapabilities.fetchModels')} isDisabled={disabled || refresh.isPending} isLoading={refresh.isPending} onPress={() => void fetchModels()}><ScanSearch className="size-3.5" /></Button></SiteTooltip>
    <SiteTooltip content={t('keyCapabilities.manageModels')}><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('keyCapabilities.manageModels')} isDisabled={disabled} onPress={() => setPanel('models')}><Boxes className="size-3.5" /></Button></SiteTooltip>
    <SiteTooltip content={t('keyCapabilities.upstreamUsage')}><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('keyCapabilities.upstreamUsage')} isDisabled={disabled} onPress={() => setPanel('usage')}><Gauge className="size-3.5" /></Button></SiteTooltip>
    {refresh.isError ? <span role="alert" className="text-[11px] text-danger">{t(`keyCapabilities.${capabilityError(refresh.error)}`)}</span> : null}
    {active && panel === 'models' ? <CredentialModelDrawer channelId={channelId} credentialId={credentialId} fetchError={refresh.isError ? capabilityError(refresh.error) : undefined} onClose={() => { setPanel(undefined); refresh.reset() }} /> : null}
    {active && panel === 'usage' ? <CredentialUsageDrawer channelId={channelId} credentialId={credentialId} onClose={() => setPanel(undefined)} /> : null}
  </>
}
