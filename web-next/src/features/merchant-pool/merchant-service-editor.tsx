import { useState } from 'react'
import { Checkbox, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Textarea } from '@heroui/react'
import { Plus, Trash2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ApiError } from '@/lib/api'
import { MerchantQuotePreview } from './merchant-quote-preview'
import { useMerchantAction, useMerchantQuote } from './merchant-pool-api'
import type { MerchantOwnedService, MerchantPool, MerchantServiceWrite } from './merchant-pool-model'
import { poolRatio, merchantProtocols, merchantTextProtocol } from './merchant-pool-model'
import { MAX_MERCHANT_MODELS, MAX_MERCHANT_MODEL_TOKENS, merchantModelField, validateMerchantService, validateMerchantServiceModels, type MerchantModelDraft, type MerchantServiceErrors } from './merchant-service-validation'
import { PoolButton as Button, PoolError, PoolField, PoolSelect } from './merchant-pool-ui'

function blankModel(): MerchantModelDraft { return { rowId: crypto.randomUUID(), model: '', upstream: '', ratio: '', context: '', maxOutput: '', streaming: true, tools: false, vision: false } }

export function MerchantServiceEditor({ service, pools, onClose }: { service?: MerchantOwnedService; pools: MerchantPool[]; onClose: () => void }) {
  const { t } = useTranslation()
  const revision = service?.draft ?? service?.published
  const [pool, setPool] = useState(String(service?.pool_id ?? pools[0]?.id ?? ''))
  const [name, setName] = useState(revision?.name ?? service?.name ?? '')
  const [description, setDescription] = useState(revision?.description ?? service?.description ?? '')
  const [baseUrl, setBaseUrl] = useState(revision?.base_url ?? '')
  const [protocol, setProtocol] = useState<MerchantServiceWrite['protocol']>(revision?.protocol ?? 'openai_chat')
  const [apiKey, setApiKey] = useState('')
  const [ratio, setRatio] = useState(poolRatio(revision?.default_ratio_micros ?? 1_000_000))
  const [concurrency, setConcurrency] = useState(revision?.concurrency ? String(revision.concurrency) : '')
  const [timeout, setTimeout] = useState(String(revision?.timeout_secs ?? 120))
  const [models, setModels] = useState<MerchantModelDraft[]>(() => revision?.models.map((model) => ({ rowId: crypto.randomUUID(), model: model.model, upstream: model.upstream_model, ratio: model.ratio_micros === null ? '' : poolRatio(model.ratio_micros), context: model.context_window === null ? '' : String(model.context_window), maxOutput: model.max_output_tokens === null ? '' : String(model.max_output_tokens), streaming: model.supports_streaming, tools: model.supports_tools, vision: model.supports_vision })) ?? [blankModel()])
  const [errors, setErrors] = useState<MerchantServiceErrors>({})
  const action = useMerchantAction<MerchantOwnedService>()
  const quote = useMerchantQuote()
  const [quotedFingerprint, setQuotedFingerprint] = useState('')
  const quoteFingerprint = JSON.stringify({ pool, ratio, models, protocol })
  const selectedPool = pools.find((item) => item.id === Number(pool))
  const errorText = (field: string) => errors[field] ? t(errors[field].key, errors[field].values) : undefined
  const inputError = (field: string) => ({ isInvalid: Boolean(errors[field]), errorMessage: errorText(field) })
  const clearFeedback = () => {
    setErrors({})
    if (action.isError) action.reset()
    if (quote.isError) quote.reset()
  }
  const editText = (setter: (value: string) => void) => (value: string) => { setter(value); clearFeedback() }
  const updateModel = (rowId: string, patch: Partial<MerchantModelDraft>) => {
    setModels((current) => current.map((model) => model.rowId === rowId ? { ...model, ...patch } : model))
    clearFeedback()
  }
  const previewQuote = async () => {
    if (quote.isPending || action.isPending) return
    const result = validateMerchantServiceModels({ pool, ratio, models, protocol, pools })
    setErrors(result.errors ?? {})
    if (result.value === undefined) return
    try { await quote.mutateAsync(result.value); setQuotedFingerprint(quoteFingerprint) } catch { /* Failed quotes preserve the complete form for correction. */ }
  }
  const save = async () => {
    if (action.isPending || quote.isPending) return
    const keyRequired = !revision?.has_api_key || baseUrl.trim() !== revision.base_url
    const result = validateMerchantService({ pool, ratio, models, protocol, pools, name, description, baseUrl, apiKey, concurrency, timeout, keyRequired })
    setErrors(result.errors ?? {})
    if (result.value === undefined) return
    try {
      await action.mutateAsync({ url: service ? `/api/merchant/services/${service.id}` : '/api/merchant/services', method: service ? 'put' : 'post', body: result.value })
      setApiKey(''); action.reset(); onClose()
    } catch { /* Sensitive fields stay only in component memory until closed. */ }
  }
  const changeProtocol = (value: string) => {
    const selected = merchantProtocols.find((item) => item.value === value)
    if (!selected) return
    setProtocol(selected.value)
    if (!merchantTextProtocol(value)) setModels((current) => current.map((model) => ({ ...model, streaming: false, tools: false, vision: false })))
    clearFeedback()
  }
  return <Drawer isOpen placement="right" size="3xl" scrollBehavior="inside" onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}>
    <DrawerContent className="min-w-0 bg-surface-1 text-foreground">
      <DrawerHeader>{t(service ? 'merchantPool.editService' : 'merchantPool.createService')}</DrawerHeader>
      <DrawerBody className="gap-4 pb-5">
        <p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.revisionHint')}</p>
        <p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.meteringHint')}</p>
        <div className="grid gap-3 sm:grid-cols-2">
          <PoolField label={t('merchantPool.name')}><Input size="sm" aria-label={t('merchantPool.name')} value={name} onValueChange={editText(setName)} maxLength={128} {...inputError('name')} /></PoolField>
          <PoolField label={t('merchantPool.pool')}>
            <PoolSelect allowEmpty={false} label={t('merchantPool.pool')} value={pool} disabled={Boolean(service)} onChange={editText(setPool)} options={pools.map((item) => ({ value: String(item.id), label: item.name }))} {...inputError('pool')} />
          </PoolField>
        </div>
        <PoolField label={t('merchantPool.description')} hint={`${t('merchantPool.market.introHint')} ${t('merchantPool.validation.multilineHint')}`}><Textarea size="sm" aria-label={t('merchantPool.description')} value={description} onValueChange={editText(setDescription)} maxLength={2048} {...inputError('description')} /></PoolField>
        <PoolField label={t('merchantPool.baseUrl')} hint={t('merchantPool.baseUrlHint')}><Input size="sm" aria-label={t('merchantPool.baseUrl')} type="url" value={baseUrl} onValueChange={editText(setBaseUrl)} autoComplete="off" placeholder="https://api.example.com/v1" {...inputError('baseUrl')} /></PoolField>
        <div className="grid gap-3 sm:grid-cols-2">
          <PoolField label={t('merchantPool.protocol')}><PoolSelect allowEmpty={false} label={t('merchantPool.protocol')} value={protocol} onChange={changeProtocol} options={[...merchantProtocols]} /></PoolField>
          <PoolField label={t('merchantPool.upstreamKey')} hint={t(revision?.has_api_key ? 'merchantPool.keyReplaceHint' : 'merchantPool.keyCreateHint')}><Input size="sm" aria-label={t('merchantPool.upstreamKey')} type="password" value={apiKey} onValueChange={editText(setApiKey)} autoComplete="new-password" {...inputError('apiKey')} /></PoolField>
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          <PoolField label={t('merchantPool.defaultRatio')} hint={selectedPool ? t('merchantPool.ratioRange', { min: poolRatio(selectedPool.min_ratio_micros), max: poolRatio(selectedPool.max_ratio_micros) }) : undefined}><Input size="sm" aria-label={t('merchantPool.defaultRatio')} inputMode="decimal" value={ratio} onValueChange={editText(setRatio)} {...inputError('ratio')} /></PoolField>
          <PoolField label={t('merchantPool.concurrency')}><Input size="sm" aria-label={t('merchantPool.concurrency')} type="number" min="1" max="10000" value={concurrency} onValueChange={editText(setConcurrency)} placeholder={t('merchantPool.noLimit')} {...inputError('concurrency')} /></PoolField>
          <PoolField label={t('merchantPool.timeout')}><Input size="sm" aria-label={t('merchantPool.timeout')} type="number" min="1" max="300" value={timeout} onValueChange={editText(setTimeout)} {...inputError('timeout')} /></PoolField>
        </div>
        <section className="grid gap-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h3 className="text-sm font-semibold">{t('merchantPool.models')}</h3>
            <Button size="sm" variant="flat" isDisabled={models.length >= MAX_MERCHANT_MODELS} onPress={() => { setModels((current) => [...current, blankModel()]); clearFeedback() }}><Plus className="size-3.5" />{t('merchantPool.addModel')}</Button>
          </div>
          {errors.models ? <PoolError message={errorText('models')} /> : null}
          <Link to="/console/models" target="_blank" rel="noopener noreferrer" className="text-xs text-primary hover:underline">{t('merchantPool.platformModelCatalog')}</Link>
          {models.map((model) => {
            const field = (name: string) => merchantModelField(model.rowId, name)
            return <article key={model.rowId} className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3">
              <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
                <PoolField label={t('merchantPool.canonicalModel')}><Input size="sm" aria-label={t('merchantPool.canonicalModel')} value={model.model} onValueChange={(value) => updateModel(model.rowId, { model: value })} {...inputError(field('model'))} /></PoolField>
                <PoolField label={t('merchantPool.upstreamModel')}><Input size="sm" aria-label={t('merchantPool.upstreamModel')} value={model.upstream} onValueChange={(value) => updateModel(model.rowId, { upstream: value })} placeholder={model.model} {...inputError(field('upstream'))} /></PoolField>
                <Button className="self-end" isIconOnly size="sm" variant="light" aria-label={t('merchantPool.removeModel')} isDisabled={models.length === 1} onPress={() => { setModels((current) => current.filter((item) => item.rowId !== model.rowId)); clearFeedback() }}><Trash2 className="size-3.5" /></Button>
              </div>
              <div className="grid gap-2 sm:grid-cols-3">
                <PoolField label={t('merchantPool.modelRatio')}><Input size="sm" aria-label={t('merchantPool.modelRatio')} value={model.ratio} inputMode="decimal" onValueChange={(value) => updateModel(model.rowId, { ratio: value })} placeholder={ratio} {...inputError(field('ratio'))} /></PoolField>
                <PoolField label={t('merchantPool.contextWindow')}><Input size="sm" aria-label={t('merchantPool.contextWindow')} type="number" min="1" max={MAX_MERCHANT_MODEL_TOKENS} value={model.context} onValueChange={(value) => updateModel(model.rowId, { context: value })} {...inputError(field('context'))} /></PoolField>
                <PoolField label={t('merchantPool.maxOutput')}><Input size="sm" aria-label={t('merchantPool.maxOutput')} type="number" min="1" max={MAX_MERCHANT_MODEL_TOKENS} value={model.maxOutput} onValueChange={(value) => updateModel(model.rowId, { maxOutput: value })} {...inputError(field('maxOutput'))} /></PoolField>
              </div>
              <div className="flex flex-wrap gap-4">
                <Checkbox size="sm" isDisabled={!merchantTextProtocol(protocol)} isSelected={merchantTextProtocol(protocol) && model.streaming} onValueChange={(value) => updateModel(model.rowId, { streaming: value })}>{t('merchantPool.streaming')}</Checkbox>
                <Checkbox size="sm" isDisabled={!merchantTextProtocol(protocol)} isSelected={merchantTextProtocol(protocol) && model.tools} onValueChange={(value) => updateModel(model.rowId, { tools: value })}>{t('merchantPool.tools')}</Checkbox>
                <Checkbox size="sm" isDisabled={!merchantTextProtocol(protocol)} isSelected={merchantTextProtocol(protocol) && model.vision} onValueChange={(value) => updateModel(model.rowId, { vision: value })}>{t('merchantPool.vision')}</Checkbox>
              </div>
            </article>
          })}
        </section>
        <Button size="sm" variant="bordered" className="justify-self-start" isLoading={quote.isPending} isDisabled={action.isPending} onPress={() => void previewQuote()}>{t('merchantPool.previewPrices')}</Button>
        {quote.isError ? <PoolError message={t(quote.error instanceof ApiError && quote.error.status === 400 ? 'merchantPool.validation.quoteUnavailable' : 'merchantPool.quoteError')} /> : null}
        {quote.data && quotedFingerprint === quoteFingerprint ? <MerchantQuotePreview quote={quote.data} /> : null}
        {selectedPool ? <p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.commissionHint', { value: selectedPool.effective_commission_bps / 100 })}</p> : null}
        {action.isError ? <PoolError message={t(action.error instanceof ApiError && action.error.status === 400 ? 'merchantPool.validation.serviceInvalid' : 'merchantPool.saveError')} /> : null}
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="flat" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button>
          <Button size="sm" color="primary" isLoading={action.isPending} isDisabled={quote.isPending} onPress={() => void save()}>{t('merchantPool.saveDraft')}</Button>
        </div>
      </DrawerBody>
    </DrawerContent>
  </Drawer>
}
