import { Button, Chip, Input } from '@heroui/react'
import { providerDisplayName } from '@/components/brand/provider-catalog'

import { useEffect, useRef, useState } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { ArrowDownToLine, CircleDollarSign, Gift, Info, Sparkles, TriangleAlert } from 'lucide-react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type {
  AdminModel,
  AdminModelPrice,
  AdminModelPriceSourceCandidate,
} from '@/lib/api/generated/types.gen'
import { ModelManagementField } from './model-management-field'
import type { ModelPriceSource } from './model-price-api'
import { ModelPriceExpressionEditor } from './model-price-expression-editor'
import {
  applyModelPriceEvidence,
  applyAllModelPriceEvidence,
  buildModelPriceDraftSchema,
  defaultModelPriceDraft,
  expressionModelPriceDraft,
  freeModelPriceDraft,
  type ModelPriceDraft,
  type ModelPriceEvidenceField,
  type StagedModelPriceDraft,
} from './model-price-form-model'

type ModelPriceFormProps = {
  candidate?: AdminModelPriceSourceCandidate
  model: AdminModel
  onCancel: () => void
  onStage: (draft: ModelPriceDraft, expectedVersion: number | null, contextWindow: number | null) => void
  price?: AdminModelPrice
  source?: ModelPriceSource
  staged?: StagedModelPriceDraft
}

const priceFields = [
  { key: 'input', translation: 'input' },
  { key: 'output', translation: 'output' },
  { key: 'cacheRead', translation: 'cacheRead' },
  { key: 'cacheCreation5m', translation: 'cacheCreation5m' },
  { key: 'cacheCreation1h', translation: 'cacheCreation1h' },
] as const

/** 编辑正式定价草稿；公开来源只能由管理员逐字段采用。 */
export function ModelPriceForm(props: ModelPriceFormProps) {
  const { t } = useTranslation()
  const schema = buildModelPriceDraftSchema({
    decimal: t('modelManagement.prices.validation.decimal'),
    free: t('modelManagement.prices.validation.free'),
    expression: t('modelManagement.prices.validation.expression'),
    expressionTooLong: t('modelManagement.prices.validation.expressionTooLong'),
    expressionValues: t('modelManagement.prices.validation.expressionValues'),
  })
  const initialDraft = props.staged?.draft ?? defaultModelPriceDraft(props.price)
  const perTokenDraft = useRef<ModelPriceDraft>(
    initialDraft.billingMode === 'per_token'
      ? initialDraft
      : { ...initialDraft, billingMode: 'per_token', expression: '' },
  )
  const expressionDraft = useRef<ModelPriceDraft>(
    initialDraft.billingMode === 'expression' ? initialDraft : expressionModelPriceDraft(),
  )
  const form = useForm<ModelPriceDraft>({
    defaultValues: initialDraft,
    resolver: zodResolver(schema),
  })
  const [expectedVersion, setExpectedVersion] = useState(props.staged?.expectedVersion ?? props.price?.version ?? null)
  const [contextWindow, setContextWindow] = useState<number | null>(props.staged?.contextWindow ?? null)

  useEffect(() => {
    const next = props.staged?.draft ?? defaultModelPriceDraft(props.price)
    if (next.billingMode === 'per_token') perTokenDraft.current = next
    if (next.billingMode === 'expression') expressionDraft.current = next
    form.reset(next)
    setExpectedVersion(props.staged?.expectedVersion ?? props.price?.version ?? null)
    setContextWindow(props.staged?.contextWindow ?? null)
  }, [form, props.model.model, props.price, props.staged])

  const billingMode = form.watch('billingMode')
  const changeBillingMode = (nextMode: ModelPriceDraft['billingMode']) => {
    const current = form.getValues()
    if (nextMode === 'free') {
      if (current.billingMode === 'per_token') perTokenDraft.current = current
      if (current.billingMode === 'expression') expressionDraft.current = current
      form.reset(freeModelPriceDraft())
      return
    }
    if (nextMode === 'expression') {
      if (current.billingMode === 'per_token') perTokenDraft.current = current
      form.reset({ ...expressionDraft.current, billingMode: 'expression' })
      return
    }
    if (current.billingMode === 'expression') expressionDraft.current = current
    form.reset({ ...perTokenDraft.current, billingMode: 'per_token', expression: '' })
  }
  const useEvidence = (field: ModelPriceEvidenceField) => {
    if (!props.candidate) return
    const current = form.getValues()
    const base = current.billingMode === 'free'
      ? { ...perTokenDraft.current, billingMode: 'per_token' as const }
      : current
    const next = applyModelPriceEvidence(base, props.candidate, field)
    perTokenDraft.current = next
    form.reset(next, { keepDefaultValues: true })
  }
  const useAllEvidence = () => {
    if (!props.candidate) return
    const current = form.getValues()
    const base = current.billingMode === 'free'
      ? { ...perTokenDraft.current, billingMode: 'per_token' as const }
      : current
    const next = applyAllModelPriceEvidence(base, props.candidate)
    perTokenDraft.current = next
    form.reset(next, { keepDefaultValues: true })
    if (props.candidate.context_window != null) {
      setContextWindow(props.candidate.context_window)
    }
  }
  const expression = form.watch('expression')
  const versionChanged = props.price !== undefined && expectedVersion !== props.price.version
  const submit = form.handleSubmit((draft) => props.onStage(draft, expectedVersion, contextWindow))

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="model-price-identity">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <h3 id="model-price-identity" className="truncate font-mono text-sm font-semibold">{props.model.model}</h3>
              <p className="mt-1 text-xs text-muted-foreground">{props.model.display_name} · {providerDisplayName(props.model.provider)}</p>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <Chip size="sm" variant="flat">{props.price ? t('modelManagement.prices.version', { version: props.price.version }) : t('modelManagement.prices.status.unpriced')}</Chip>
              {props.staged ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('modelManagement.prices.status.staged')}</Chip> : null}
            </div>
          </div>
          <div
            role="radiogroup"
            aria-label={t('modelManagement.prices.form.billingMode')}
            className="grid grid-cols-3 gap-1 rounded-lg border border-[var(--hairline)] bg-surface-2/55 p-1"
          >
            {([
              ['per_token', CircleDollarSign],
              ['free', Gift],
              ['expression', Sparkles],
            ] as const).map(([mode, Icon]) => (
              <Button
                key={mode}
                type="button"
                size="sm"
                variant="light"
                role="radio"
                aria-checked={billingMode === mode}
                className={billingMode === mode ? 'bg-surface-1 hover:bg-surface-1' : undefined}
                onClick={() => changeBillingMode(mode)}
              >
                <Icon className="size-3.5" aria-hidden="true" />
                {t(`modelManagement.prices.billing.${mode}`)}
              </Button>
            ))}
          </div>
          <p className="flex items-start gap-2 text-xs leading-5 text-muted-foreground">
            <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            {t(billingMode === 'free'
              ? 'modelManagement.prices.form.freeHint'
              : billingMode === 'expression'
                ? 'modelManagement.prices.form.expressionHint'
                : 'modelManagement.prices.form.perTokenHint')}
          </p>
          {versionChanged ? (
            <div className="flex flex-col gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs text-warning sm:flex-row sm:items-center sm:justify-between">
              <span>{t('modelManagement.prices.form.versionChanged', { current: props.price?.version, expected: expectedVersion ?? t('modelManagement.prices.form.newVersion') })}</span>
              <Button type="button" size="sm" variant="bordered" onClick={() => setExpectedVersion(props.price?.version ?? null)}>
                {t('modelManagement.prices.actions.useCurrentVersion')}
              </Button>
            </div>
          ) : null}
        </section>

        {billingMode === 'expression' ? (
          <ModelPriceExpressionEditor
            key={props.model.model}
            candidate={props.candidate}
            value={expression}
            error={form.formState.errors.expression?.message}
            onChange={(value) => form.setValue('expression', value, { shouldDirty: true, shouldValidate: true })}
          />
        ) : (
          <>
            <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-price-values">
              <div>
                <h3 id="model-price-values" className="text-xs font-semibold">{t('modelManagement.prices.form.values')}</h3>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.form.valuesHint')}</p>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                {priceFields.map(({ key, translation }) => (
                  <ModelManagementField
                    key={key}
                    id={`model-price-${key}`}
                    label={t(`modelManagement.prices.fields.${translation}`)}
                    hint={t('modelManagement.prices.form.priceUnit')}
                    error={form.formState.errors[key]?.message}
                  >
                    <Input
                      id={`model-price-${key}`}
                      type="text"
                      size="sm"
                      inputMode="decimal"
                      autoComplete="off"
                      isDisabled={billingMode === 'free'}
                      isInvalid={!!form.formState.errors[key]}
                      classNames={{ input: 'font-mono tabular-nums' }}
                      {...form.register(key)}
                    />
                  </ModelManagementField>
                ))}
              </div>
            </section>

            <PriceEvidence
              candidate={props.candidate}
              contextWindow={contextWindow}
              source={props.source}
              onUse={useEvidence}
              onUseAll={useAllEvidence}
              onUseContext={() => setContextWindow(props.candidate?.context_window ?? null)}
            />
          </>
        )}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" onClick={props.onCancel}>{t('modelManagement.actions.cancel')}</Button>
        <Button type="submit" color="primary" isDisabled={versionChanged}>{t('modelManagement.prices.actions.stage')}</Button>
      </div>
    </form>
  )
}

function PriceEvidence(props: {
  candidate?: AdminModelPriceSourceCandidate
  contextWindow: number | null
  onUse: (field: ModelPriceEvidenceField) => void
  onUseAll: () => void
  onUseContext: () => void
  source?: ModelPriceSource
}) {
  const { t } = useTranslation()
  const candidate = props.candidate
  const sourceName = props.source
    ? t(`modelManagement.prices.sources.${props.source}`)
    : t('modelManagement.prices.sources.public')
  if (!candidate) {
    return (
      <section className="border-t border-[var(--hairline)] pt-5" aria-labelledby="model-price-evidence">
        <h3 id="model-price-evidence" className="text-xs font-semibold">{t('modelManagement.prices.evidence.title', { source: sourceName })}</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.evidence.none', { source: sourceName })}</p>
      </section>
    )
  }
  const evidence = [
    { field: 'input', label: 'input', value: candidate.costs.input },
    { field: 'output', label: 'output', value: candidate.costs.output },
    { field: 'cacheRead', label: 'cacheRead', value: candidate.costs.cache_read },
    { field: 'cacheCreation5m', label: 'cacheCreation5m', value: candidate.costs.cache_creation_5m },
    { field: 'cacheCreation1h', label: 'cacheCreation1h', value: candidate.costs.cache_creation_1h },
  ] as const
  return (
    <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-price-evidence">
      <div>
        <div className="flex flex-wrap items-center gap-2">
          <h3 id="model-price-evidence" className="text-xs font-semibold">{t('modelManagement.prices.evidence.title', { source: sourceName })}</h3>
          <Chip size="sm" variant="flat">{candidate.provider_name}</Chip>
          {candidate.has_tiered_pricing ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('modelManagement.prices.status.review')}</Chip> : null}
          {candidate.source_deprecated ? <Chip className="bg-destructive/10 text-destructive" size="sm" variant="flat">{t('modelManagement.prices.evidence.deprecated')}</Chip> : null}
          {candidate.source_deprecation_date ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('modelManagement.prices.evidence.deprecationDate', { date: candidate.source_deprecation_date })}</Chip> : null}
          <Button type="button" size="sm" variant="bordered" className="ml-auto" onClick={props.onUseAll}>
            <ArrowDownToLine className="size-3.5" aria-hidden="true" />
            {t('modelManagement.prices.actions.useAllEvidence')}
          </Button>
        </div>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.evidence.disclaimer')}</p>
      </div>
      {candidate.has_tiered_pricing ? (
        <div className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {t('modelManagement.prices.evidence.tieredWarning')}
        </div>
      ) : null}
      <dl className="grid gap-2 sm:grid-cols-2">
        {candidate.context_window != null ? (
          <div className="flex min-h-14 items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2 sm:col-span-2">
            <div className="min-w-0">
              <dt className="text-[0.6875rem] text-muted-foreground">{t('modelManagement.prices.fields.contextWindow')}</dt>
              <dd className="mt-1 truncate font-mono text-xs tabular-nums">
                {new Intl.NumberFormat().format(candidate.context_window)} tokens
                {props.contextWindow === candidate.context_window ? ` · ${t('modelManagement.prices.evidence.staged')}` : ''}
              </dd>
            </div>
            <Button isIconOnly type="button" size="sm" variant="light" aria-label={t('modelManagement.prices.evidence.useContext')} onClick={props.onUseContext}>
              <ArrowDownToLine className="size-3.5" aria-hidden="true" />
            </Button>
          </div>
        ) : null}
        {evidence.map(({ field, label, value }) => (
          <div key={field} className="flex min-h-14 items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2">
            <div className="min-w-0">
              <dt className="text-[0.6875rem] text-muted-foreground">{t(`modelManagement.prices.fields.${label}`)}</dt>
              <dd className="mt-1 truncate font-mono text-xs tabular-nums">{value === null ? t('modelManagement.prices.evidence.missing') : `$${value}`}</dd>
            </div>
            <Button isIconOnly type="button" size="sm" variant="light" isDisabled={value === null} aria-label={t('modelManagement.prices.evidence.use', { field: t(`modelManagement.prices.fields.${label}`) })} onClick={() => props.onUse(field)}>
              <ArrowDownToLine className="size-3.5" aria-hidden="true" />
            </Button>
          </div>
        ))}
        {candidate.costs.cache_write !== null ? (
          <div className="flex min-h-14 items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2 sm:col-span-2">
            <div className="min-w-0">
              <dt className="text-[0.6875rem] text-muted-foreground">{t('modelManagement.prices.fields.cacheWriteEvidence')}</dt>
              <dd className="mt-1 truncate font-mono text-xs tabular-nums">${candidate.costs.cache_write}</dd>
            </div>
            <span className="max-w-52 text-right text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.evidence.cacheWriteBoundary')}</span>
          </div>
        ) : null}
      </dl>
      <p className="text-[0.6875rem] text-muted-foreground">
        {t('modelManagement.prices.evidence.source', {
          model: candidate.source_model,
          name: candidate.source_name,
          updated: candidate.last_updated ?? t('modelManagement.prices.evidence.unknownDate'),
        })}
      </p>
    </section>
  )
}
