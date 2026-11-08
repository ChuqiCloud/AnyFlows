import { useState } from 'react'
import { Button, Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useMerchantRoutePreview } from './merchant-pool-api'
import { poolRatio, poolTimestamp, type MerchantPoolRouteWrite, type MerchantRoutePreviewRequest } from './merchant-pool-model'
import { PoolEmpty, PoolError, PoolField, PoolSelect } from './merchant-pool-ui'

export function MerchantRoutePreviewPanel({ getRoute, configurationKey }: { getRoute: () => MerchantPoolRouteWrite | undefined; configurationKey: string }) {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const [model, setModel] = useState('')
  const [protocol, setProtocol] = useState<MerchantRoutePreviewRequest['protocol']>('openai_chat')
  const [streaming, setStreaming] = useState('true')
  const [input, setInput] = useState('1000')
  const [output, setOutput] = useState('1000')
  const [invalid, setInvalid] = useState(false)
  const [snapshotKey, setSnapshotKey] = useState('')
  const preview = useMerchantRoutePreview()
  const currentKey = JSON.stringify({ configurationKey, model, protocol, streaming, input, output })
  const requestPreview = async () => {
    const route = getRoute()
    const inputTokens = /^\d+$/.test(input) ? Number(input) : NaN
    const outputTokens = /^\d+$/.test(output) ? Number(output) : NaN
    if (!route || !model.trim() || !Number.isSafeInteger(inputTokens) || !Number.isSafeInteger(outputTokens) || inputTokens < 0 || outputTokens < 0) { setInvalid(true); return }
    try { await preview.mutateAsync({ route, model: model.trim(), protocol, streaming: streaming === 'true', input_tokens: inputTokens, output_tokens: outputTokens }); setSnapshotKey(currentKey); setInvalid(false) } catch { /* A failed preview leaves the route and preview inputs intact. */ }
  }
  return <section className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3"><h3 className="text-sm font-semibold">{t('merchantPool.routePreview')}</h3><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.model')}><Input aria-label={t('merchantPool.model')} value={model} onValueChange={setModel} size="sm" /></PoolField><PoolField label={t('merchantPool.protocol')}><PoolSelect label={t('merchantPool.protocol')} allowEmpty={false} value={protocol} onChange={(value) => { if (value === 'openai_chat' || value === 'openai_responses') setProtocol(value) }} options={[{ value: 'openai_chat', label: 'OpenAI Chat Completions' }, { value: 'openai_responses', label: 'OpenAI Responses' }]} /></PoolField><PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} value={streaming} allowEmpty={false} onChange={setStreaming} options={[{ value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField><div className="grid grid-cols-2 gap-2"><PoolField label={t('merchantPool.previewInputTokens')}><Input aria-label={t('merchantPool.previewInputTokens')} size="sm" type="number" min="0" step="1" value={input} onValueChange={setInput} /></PoolField><PoolField label={t('merchantPool.previewOutputTokens')}><Input aria-label={t('merchantPool.previewOutputTokens')} size="sm" type="number" min="0" step="1" value={output} onValueChange={setOutput} /></PoolField></div></div><Button size="sm" variant="bordered" className="justify-self-start" isLoading={preview.isPending} onPress={() => void requestPreview()}>{t('merchantPool.previewCandidates')}</Button>{invalid ? <PoolError message={t('merchantPool.invalidPreview')} /> : null}{preview.isError ? <PoolError message={t('merchantPool.previewError')} /> : null}{preview.data && snapshotKey === currentKey ? <div className="grid gap-2">{preview.data.items.length === 0 ? <PoolEmpty>{t('merchantPool.previewEmpty')}</PoolEmpty> : preview.data.items.map((item, index) => <article key={`${item.merchant_id}-${item.service_id}-${index}`} className="grid gap-1 rounded-lg bg-surface-2 px-3 py-2 text-xs"><div className="flex flex-wrap justify-between gap-2"><span className="font-medium">{index + 1}. {item.merchant_name} · {item.service_name}</span><span className={item.eligible ? 'text-success' : 'text-warning'}>{t(item.eligible ? 'merchantPool.previewEligible' : 'merchantPool.previewIneligible')}</span></div><div className="flex flex-wrap gap-3 text-muted-foreground"><span>{t('merchantPool.ratioValue', { value: poolRatio(item.sales_ratio_micros) })}</span><span>{item.estimated_quota === null ? t('merchantPool.noPrice') : t('merchantPool.estimatedCharge', { value: formatQuota(item.estimated_quota) })}</span></div>{item.reason ? <p className="text-warning">{t(`merchantPool.previewReasons.${item.reason}`, { defaultValue: item.reason })}</p> : null}</article>)}<p className="text-[0.6875rem] text-muted-foreground">{t('merchantPool.statsAsOf', { value: poolTimestamp(preview.data.as_of, i18n.language) })}</p></div> : null}<p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.routePreviewHint')}</p></section>
}
