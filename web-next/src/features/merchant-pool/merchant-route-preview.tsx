import { useState } from 'react'
import { Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useMerchantRoutePreview } from './merchant-pool-api'
import { poolRatio, poolTimestamp, merchantProtocols, merchantTextProtocol, type MerchantPoolRouteWrite, type MerchantRoutePreviewRequest } from './merchant-pool-model'
import { MerchantManagementTable } from './merchant-management-tables'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolSelect } from './merchant-pool-ui'

export function MerchantRoutePreviewPanel({ getRoute, configurationKey }: { getRoute: () => MerchantPoolRouteWrite | undefined; configurationKey: string }) {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const [model, setModel] = useState('')
  const [protocol, setProtocol] = useState<MerchantRoutePreviewRequest['protocol']>('openai_chat')
  const [streaming, setStreaming] = useState('true')
  const [input, setInput] = useState('1000')
  const [output, setOutput] = useState('1000')
  const [duration, setDuration] = useState('8')
  const [resolution, setResolution] = useState<'480p' | '720p' | '1080p'>('480p')
  const [invalid, setInvalid] = useState(false)
  const [snapshotKey, setSnapshotKey] = useState('')
  const preview = useMerchantRoutePreview()
  const currentKey = JSON.stringify({ configurationKey, model, protocol, streaming, input, output, duration, resolution })
  const requestPreview = async () => {
    const route = getRoute()
    const inputTokens = /^\d+$/.test(input) ? Number(input) : NaN
    const outputTokens = /^\d+$/.test(output) ? Number(output) : NaN
    const seconds = /^\d+$/.test(duration) ? Number(duration) : NaN
    if (protocol === 'xai_video' && (!Number.isSafeInteger(seconds) || seconds < 1 || seconds > 15)) { setInvalid(true); return }
    if (!route || !model.trim() || !Number.isSafeInteger(inputTokens) || !Number.isSafeInteger(outputTokens) || inputTokens < 0 || outputTokens < 0) { setInvalid(true); return }
    try { await preview.mutateAsync({ route, model: model.trim(), protocol, streaming: merchantTextProtocol(protocol) && streaming === 'true', input_tokens: inputTokens, output_tokens: outputTokens, ...(protocol === 'xai_video' ? { duration_seconds: seconds, resolution } : {}) }); setSnapshotKey(currentKey); setInvalid(false) } catch { /* A failed preview leaves the route and preview inputs intact. */ }
  }
  return <section className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3"><h3 className="text-sm font-semibold">{t('merchantPool.routePreview')}</h3><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.model')}><Input aria-label={t('merchantPool.model')} value={model} onValueChange={setModel} size="sm" /></PoolField><PoolField label={t('merchantPool.protocol')}><PoolSelect label={t('merchantPool.protocol')} allowEmpty={false} value={protocol} onChange={(value) => { const selected = merchantProtocols.find((item) => item.value === value); if (selected) setProtocol(selected.value) }} options={[...merchantProtocols]} /></PoolField><PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} disabled={!merchantTextProtocol(protocol)} value={merchantTextProtocol(protocol) ? streaming : 'false'} allowEmpty={false} onChange={setStreaming} options={[{ value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField>{protocol === 'xai_video' ? <div className="grid grid-cols-2 gap-2"><PoolField label={t('merchantPool.videoDuration')}><Input aria-label={t('merchantPool.videoDuration')} size="sm" type="number" min="1" max="15" step="1" value={duration} onValueChange={setDuration} /></PoolField><PoolField label={t('merchantPool.videoResolution')}><PoolSelect label={t('merchantPool.videoResolution')} allowEmpty={false} value={resolution} onChange={(value) => { if (value === '480p' || value === '720p' || value === '1080p') setResolution(value) }} options={['480p', '720p', '1080p'].map((value) => ({ value, label: value }))} /></PoolField></div> : <div className="grid grid-cols-2 gap-2"><PoolField label={t('merchantPool.previewInputTokens')}><Input aria-label={t('merchantPool.previewInputTokens')} size="sm" type="number" min="0" step="1" value={input} onValueChange={setInput} /></PoolField><PoolField label={t('merchantPool.previewOutputTokens')}><Input aria-label={t('merchantPool.previewOutputTokens')} size="sm" type="number" min="0" step="1" value={output} onValueChange={setOutput} /></PoolField></div>}</div><Button size="sm" variant="bordered" className="justify-self-start" isLoading={preview.isPending} onPress={() => void requestPreview()}>{t('merchantPool.previewCandidates')}</Button>{invalid ? <PoolError message={t('merchantPool.invalidPreview')} /> : null}{preview.isError ? <PoolError message={t('merchantPool.previewError')} /> : null}{preview.data && snapshotKey === currentKey ? <div className="grid gap-2">{preview.data.items.length === 0 ? <PoolEmpty>{t('merchantPool.previewEmpty')}</PoolEmpty> : <MerchantManagementTable label={t('merchantPool.routePreview')} columns={['merchantService', 'ratio', 'amount', 'status', 'reason']}>{preview.data.items.map((item, index) => <tr key={`${item.merchant_id}-${item.service_id}-${index}`} className="border-b border-[var(--hairline)]"><td className="whitespace-nowrap px-2 py-2">{index + 1}. {item.merchant_name} · {item.service_name}</td><td className="whitespace-nowrap px-2 py-2">{poolRatio(item.sales_ratio_micros)}×</td><td className="whitespace-nowrap px-2 py-2">{item.estimated_quota === null ? t('merchantPool.noPrice') : t('merchantPool.estimatedCharge', { value: formatQuota(item.estimated_quota) })}</td><td className={`whitespace-nowrap px-2 py-2 ${item.eligible ? 'text-success' : 'text-warning'}`}>{t(item.eligible ? 'merchantPool.previewEligible' : 'merchantPool.previewIneligible')}</td><td className="whitespace-nowrap px-2 py-2 text-warning">{item.reason ? t(`merchantPool.previewReasons.${item.reason}`, { defaultValue: item.reason }) : '—'}</td></tr>)}</MerchantManagementTable>}<p className="text-[0.6875rem] text-muted-foreground">{t('merchantPool.statsAsOf', { value: poolTimestamp(preview.data.as_of, i18n.language) })}</p></div> : null}<p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.routePreviewHint')}</p></section>
}
