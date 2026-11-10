import { useState } from 'react'
import { Checkbox, Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction } from './merchant-pool-api'
import { useMerchantPayoutSettings } from './merchant-extension-api'
import type { MerchantPayoutSettings, MerchantPayoutSettingsWrite } from './merchant-extension-model'
import { positiveInteger } from './merchant-pool-model'
import { merchantTextError } from './merchant-form-validation'
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
  const [errors, setErrors] = useState<Record<string, { key: string; values?: Record<string, string | number> }>>({})
  const action = useMerchantAction()
  const errorText = (field: string) => errors[field] ? t(`merchantPool.validation.${errors[field].key}`, errors[field].values) : undefined
  const inputError = (field: string) => ({ isInvalid: Boolean(errors[field]), errorMessage: errorText(field) })
  const clearErrors = (...fields: string[]) => setErrors((current) => { const next = { ...current }; fields.forEach((field) => delete next[field]); return next })
  const edit = (field: string, setter: (value: string) => void, dependent: string[] = []) => (value: string) => { setter(value); clearErrors(field, ...dependent); if (action.isError) action.reset() }
  const updateEnabled = (value: boolean) => { setEnabled(value); clearErrors('gateway', 'signingKey'); if (action.isError) action.reset() }
  const updateAutoApprove = (value: boolean) => { setAutoApprove(value); if (action.isError) action.reset() }
  const updateClearSecret = (value: boolean) => { setClearSecret(value); if (value) setSecret(''); clearErrors('signingKey'); if (action.isError) action.reset() }
  const save = async () => {
    if (action.isPending) return
    const units = positiveInteger(quota), min = positiveInteger(minimum), max = positiveInteger(maximum), cap = positiveInteger(daily)
    const nextErrors: typeof errors = {}
    if (!units) nextErrors.quota = { key: 'payoutQuotaRequired' }
    if (!min || min > 1_000_000_000) nextErrors.minimum = { key: 'payoutAmountRange', values: { max: '1000000000' } }
    if (!max || max > 1_000_000_000) nextErrors.maximum = { key: 'payoutAmountRange', values: { max: '1000000000' } }
    if (!cap || cap > 100_000_000_000) nextErrors.daily = { key: 'payoutAmountRange', values: { max: '100000000000' } }
    if (min && max && max < min) nextErrors.maximum = { key: 'payoutMaximumBelowMinimum' }
    if (max && cap && cap < max) nextErrors.daily = { key: 'dailyLimitBelowMaximum' }
    const gatewayValue = gateway.trim()
    let urlValid = false
    try { const url = new URL(gatewayValue); urlValid = url.protocol === 'https:' && Boolean(url.host) && !url.username && !url.password && !gatewayValue.includes('?') && !gatewayValue.includes('#') && !/\p{Cc}/u.test(gatewayValue) && new TextEncoder().encode(gatewayValue).byteLength <= 2048 } catch { /* Keep the address in the form for correction. */ }
    if ((enabled || gatewayValue !== '') && !urlValid) nextErrors.gateway = { key: 'gatewayUrl' }
    if (!/^[A-Z]{3}$/.test(currency) || !['USD','CNY','EUR','GBP','HKD','SGD','AUD','CAD','CHF','NZD','INR','BRL'].includes(currency)) nextErrors.currency = { key: 'payoutCurrencyInvalid' }
    const secretValue = secret.trim()
    if (clearSecret && secretValue) nextErrors.signingKey = { key: 'signingKeyClearConflict' }
    else if (secretValue && (merchantTextError(secretValue, 1024, true) || new TextEncoder().encode(secretValue).byteLength < 32)) nextErrors.signingKey = { key: 'signingKeyBytes', values: { min: 32, max: 1024 } }
    else if ((enabled || !settings.signing_key_configured) && (clearSecret || !secretValue) && !(settings.signing_key_configured && !clearSecret)) nextErrors.signingKey = { key: 'signingKeyRequired' }
    if (Object.keys(nextErrors).length) { setErrors(nextErrors); return }
    const body: MerchantPayoutSettingsWrite = { expected_version: settings.version, enabled, auto_approve_enabled: autoApprove, gateway_url: gatewayValue, currency, quota_per_unit: units!, min_amount_minor: min!, max_amount_minor: max!, daily_limit_minor: cap!, clear_signing_key: clearSecret, ...(secretValue && !clearSecret ? { signing_key: secretValue } : {}) }
    try { await action.mutateAsync({ url: '/api/admin/merchant-payout-settings', method: 'put', body }); setSecret(''); action.reset(); setErrors({}) } catch { /* Preserve draft and version; never silently overwrite concurrent settings. */ }
  }
  return <section className="grid gap-3"><h3 className="text-sm font-semibold">{t('merchantPool.adminTabs.payoutSettings')}</h3><p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.payoutGatewayHint')}</p><PayoutGatewayHelp /><div className="flex flex-wrap gap-4"><Checkbox size="sm" isSelected={enabled} onValueChange={updateEnabled}>{t('merchantPool.enablePayout')}</Checkbox><Checkbox size="sm" isSelected={autoApprove} onValueChange={updateAutoApprove}>{t('merchantPool.autoApprovePayout')}</Checkbox></div><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.payoutGateway')}><Input aria-label={t('merchantPool.payoutGateway')} size="sm" type="url" autoComplete="off" value={gateway} onValueChange={edit('gateway', setGateway)} {...inputError('gateway')} /></PoolField><PoolField label={t('merchantPool.payoutCurrency')}><PoolSelect label={t('merchantPool.payoutCurrency')} allowEmpty={false} value={currency} onChange={(value) => { setCurrency(value); clearErrors('currency'); if (action.isError) action.reset() }} {...inputError('currency')} options={['USD', 'CNY', 'EUR', 'GBP', 'HKD', 'SGD', 'AUD', 'CAD', 'CHF', 'NZD', 'INR', 'BRL'].map((value) => ({ value, label: value }))} /></PoolField><PoolField label={t('merchantPool.payoutQuotaPerUnit')} hint={t('merchantPool.payoutConversionHint')}><Input aria-label={t('merchantPool.payoutQuotaPerUnit')} size="sm" inputMode="numeric" value={quota} onValueChange={edit('quota', setQuota)} {...inputError('quota')} /></PoolField><PoolField label={t('merchantPool.payoutMinimum')}><Input aria-label={t('merchantPool.payoutMinimum')} size="sm" inputMode="numeric" value={minimum} onValueChange={edit('minimum', setMinimum, ['maximum'])} {...inputError('minimum')} /></PoolField><PoolField label={t('merchantPool.payoutMaximum')}><Input aria-label={t('merchantPool.payoutMaximum')} size="sm" inputMode="numeric" value={maximum} onValueChange={edit('maximum', setMaximum, ['minimum', 'daily'])} {...inputError('maximum')} /></PoolField><PoolField label={t('merchantPool.payoutDailyLimit')}><Input aria-label={t('merchantPool.payoutDailyLimit')} size="sm" inputMode="numeric" value={daily} onValueChange={edit('daily', setDaily, ['maximum'])} {...inputError('daily')} /></PoolField></div><PoolField label={t('merchantPool.payoutSigningKey')} hint={t(settings.signing_key_configured ? 'merchantPool.payoutSecretPreserve' : 'merchantPool.payoutSecretRequired')}><Input aria-label={t('merchantPool.payoutSigningKey')} size="sm" type="password" autoComplete="new-password" isDisabled={clearSecret} value={secret} onValueChange={edit('signingKey', setSecret)} {...inputError('signingKey')} /></PoolField><Checkbox size="sm" isSelected={clearSecret} onValueChange={updateClearSecret}>{t('merchantPool.clearSigningKey')}</Checkbox>{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<Button size="sm" className="justify-self-end" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></section>
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
