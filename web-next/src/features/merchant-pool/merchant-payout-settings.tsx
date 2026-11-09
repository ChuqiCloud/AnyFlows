import { useState } from 'react'
import { Checkbox, Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction } from './merchant-pool-api'
import { useMerchantPayoutSettings } from './merchant-extension-api'
import type { MerchantPayoutSettings, MerchantPayoutSettingsWrite } from './merchant-extension-model'
import { positiveInteger } from './merchant-pool-model'
import { PoolButton as Button, PoolError, PoolField, PoolLoading, PoolSelect } from './merchant-pool-ui'

export function MerchantPayoutSettingsPanel() {
  const query = useMerchantPayoutSettings()
  return query.isPending ? <PoolLoading /> : query.isError ? <PoolError onRetry={() => void query.refetch()} /> : <PayoutSettingsForm key={query.data.version} settings={query.data} />
}

function PayoutSettingsForm({ settings }: { settings: MerchantPayoutSettings }) {
  const { t } = useTranslation()
  const [enabled, setEnabled] = useState(settings.enabled)
  const [autoApprove, setAutoApprove] = useState(settings.auto_approve_enabled)
  const [gateway, setGateway] = useState(settings.gateway_url)
  const [currency, setCurrency] = useState(settings.currency)
  const [quota, setQuota] = useState(String(settings.quota_per_unit))
  const [minimum, setMinimum] = useState(String(settings.min_amount_minor))
  const [maximum, setMaximum] = useState(String(settings.max_amount_minor))
  const [daily, setDaily] = useState(String(settings.daily_limit_minor))
  const [secret, setSecret] = useState('')
  const [clearSecret, setClearSecret] = useState(false)
  const [invalid, setInvalid] = useState(false)
  const action = useMerchantAction()
  const save = async () => {
    const units = positiveInteger(quota), min = positiveInteger(minimum), max = positiveInteger(maximum), cap = positiveInteger(daily)
    let urlValid = false
    try { const url = new URL(gateway); urlValid = url.protocol === 'https:' && !url.username && !url.password && !url.search && !url.hash } catch { /* Keep invalid URL for correction. */ }
    if (!/^[A-Z]{3}$/.test(currency) || !units || !min || !max || !cap || max < min || cap < min || (enabled && (!urlValid || clearSecret || (!settings.signing_key_configured && !secret.trim()))) || (gateway !== '' && !urlValid)) { setInvalid(true); return }
    const body: MerchantPayoutSettingsWrite = { expected_version: settings.version, enabled, auto_approve_enabled: autoApprove, gateway_url: gateway.trim(), currency, quota_per_unit: units, min_amount_minor: min, max_amount_minor: max, daily_limit_minor: cap, clear_signing_key: clearSecret, ...(secret.trim() && !clearSecret ? { signing_key: secret.trim() } : {}) }
    try { await action.mutateAsync({ url: '/api/admin/merchant-payout-settings', method: 'put', body }); setSecret(''); action.reset(); setInvalid(false) } catch { /* Preserve draft and version; never silently overwrite concurrent settings. */ }
  }
  return <section className="grid gap-3"><h3 className="text-sm font-semibold">{t('merchantPool.adminTabs.payoutSettings')}</h3><p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.payoutGatewayHint')}</p><PayoutGatewayHelp /><div className="flex flex-wrap gap-4"><Checkbox size="sm" isSelected={enabled} onValueChange={setEnabled}>{t('merchantPool.enablePayout')}</Checkbox><Checkbox size="sm" isSelected={autoApprove} onValueChange={setAutoApprove}>{t('merchantPool.autoApprovePayout')}</Checkbox></div><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.payoutGateway')}><Input aria-label={t('merchantPool.payoutGateway')} size="sm" type="url" autoComplete="off" value={gateway} onValueChange={setGateway} /></PoolField><PoolField label={t('merchantPool.payoutCurrency')}><PoolSelect label={t('merchantPool.payoutCurrency')} allowEmpty={false} value={currency} onChange={setCurrency} options={['USD', 'CNY', 'EUR', 'GBP', 'HKD', 'SGD', 'AUD', 'CAD', 'CHF', 'NZD', 'INR', 'BRL'].map((value) => ({ value, label: value }))} /></PoolField><PoolField label={t('merchantPool.payoutQuotaPerUnit')} hint={t('merchantPool.payoutConversionHint')}><Input aria-label={t('merchantPool.payoutQuotaPerUnit')} size="sm" inputMode="numeric" value={quota} onValueChange={setQuota} /></PoolField><PoolField label={t('merchantPool.payoutMinimum')}><Input aria-label={t('merchantPool.payoutMinimum')} size="sm" inputMode="numeric" value={minimum} onValueChange={setMinimum} /></PoolField><PoolField label={t('merchantPool.payoutMaximum')}><Input aria-label={t('merchantPool.payoutMaximum')} size="sm" inputMode="numeric" value={maximum} onValueChange={setMaximum} /></PoolField><PoolField label={t('merchantPool.payoutDailyLimit')}><Input aria-label={t('merchantPool.payoutDailyLimit')} size="sm" inputMode="numeric" value={daily} onValueChange={setDaily} /></PoolField></div><PoolField label={t('merchantPool.payoutSigningKey')} hint={t(settings.signing_key_configured ? 'merchantPool.payoutSecretPreserve' : 'merchantPool.payoutSecretRequired')}><Input aria-label={t('merchantPool.payoutSigningKey')} size="sm" type="password" autoComplete="new-password" isDisabled={clearSecret} value={secret} onValueChange={setSecret} /></PoolField><Checkbox size="sm" isSelected={clearSecret} onValueChange={setClearSecret}>{t('merchantPool.clearSigningKey')}</Checkbox>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<Button size="sm" className="justify-self-end" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></section>
}


function PayoutGatewayHelp() {
  const { t } = useTranslation()
  const codeClass = 'min-w-0 overflow-x-auto rounded-md bg-surface-2 px-3 py-2 font-mono text-[11px] leading-5'
  return <details className="min-w-0 border-b border-[var(--hairline)] pb-3 text-xs"><summary className="w-fit cursor-pointer font-medium">{t('merchantPool.payoutProtocol.title')}</summary><div className="mt-3 grid min-w-0 gap-2 leading-5"><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.connection')}</p><h4 className="mt-1 font-medium">{t('merchantPool.payoutProtocol.requestsTitle')}</h4><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.requests')}</p><pre className={codeClass}><code>{payoutRequestExample}</code></pre><h4 className="mt-1 font-medium">{t('merchantPool.payoutProtocol.signingTitle')}</h4><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.signing')}</p><pre className={codeClass}><code>{payoutSignatureExample}</code></pre><h4 className="mt-1 font-medium">{t('merchantPool.payoutProtocol.responseTitle')}</h4><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.response')}</p><pre className={codeClass}><code>{payoutReceiptExample}</code></pre><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.states')}</p><p className="text-muted-foreground">{t('merchantPool.payoutProtocol.idempotency')}</p></div></details>
}

const payoutRequestExample = `POST {base}/payouts
{"event_key":"payout-example-001","amount_minor":1250,"currency":"USD","account":"opaque-account-token"}

POST {base}/payouts/query
{"event_key":"payout-example-001"}`

const payoutSignatureExample = `import { createHmac } from 'node:crypto';

function signedJson(payload, eventKey, key) {
  const timestamp = String(Math.floor(Date.now() / 1000));
  const body = Buffer.from(JSON.stringify(payload), 'utf8');
  const signature = createHmac('sha256', key)
    .update(timestamp + '\n', 'utf8').update(body).digest('hex');
  return { body, headers: {
    'Content-Type': 'application/json',
    'Idempotency-Key': eventKey,
    'X-AnyFlows-Timestamp': timestamp,
    'X-AnyFlows-Signature': signature,
  }};
}`

const payoutReceiptExample = `HTTP/1.1 200 OK
Content-Type: application/json
X-AnyFlows-Timestamp: <UTC Unix seconds>
X-AnyFlows-Signature: <64-character lowercase HMAC hex>

{"event_key":"payout-example-001","amount_minor":1250,"currency":"USD","status":"paid","reference":"gateway-transfer-001"}`
