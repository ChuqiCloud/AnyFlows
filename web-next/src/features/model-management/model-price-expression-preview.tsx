import { Button, Chip, Input } from '@heroui/react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Calculator, Check, Info, LoaderCircle, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminModelPriceExpressionPreviewResponse } from '@/lib/api/generated/types.gen'
import { ModelManagementField } from './model-management-field'
import {
  DEFAULT_EXPRESSION_PREVIEW_RATIOS,
  DEFAULT_EXPRESSION_PREVIEW_USAGE,
  buildExpressionPreviewRequest,
  expressionPreviewResultFields,
  type ExpressionPreviewRatiosDraft,
  type ExpressionPreviewSemantics,
  type ExpressionPreviewUsageDraft,
} from './model-price-expression-preview-model'
import { modelPriceErrorCode, usePreviewAdminModelPriceExpression } from './model-price-api'
import { expressionSourceBytes, expressionVariables } from './model-price-expression-model'

type ModelPriceExpressionPreviewProps = {
  onUseVisual: () => void
  source: string
}

const usageFields = [
  ['inputTokens', 'inputTokens', 'numeric'],
  ['outputTokens', 'outputTokens', 'numeric'],
  ['cacheReadTokens', 'cacheReadTokens', 'numeric'],
  ['cacheCreation5mTokens', 'cacheCreation5mTokens', 'numeric'],
  ['cacheCreation1hTokens', 'cacheCreation1hTokens', 'numeric'],
] as const

const ratioFields = [
  ['group', 'group'],
  ['groupModel', 'groupModel'],
  ['peak', 'peak'],
] as const

/** 结构化输入表达式试算；金额和 quota 始终来自服务端 Decimal 结果。 */
export function ModelPriceExpressionPreview(props: ModelPriceExpressionPreviewProps) {
  const { t } = useTranslation()
  const { isPending, mutateAsync, reset } = usePreviewAdminModelPriceExpression()
  const [usage, setUsage] = useState<ExpressionPreviewUsageDraft>(() => ({ ...DEFAULT_EXPRESSION_PREVIEW_USAGE }))
  const [ratios, setRatios] = useState<ExpressionPreviewRatiosDraft>(() => ({ ...DEFAULT_EXPRESSION_PREVIEW_RATIOS }))
  const [result, setResult] = useState<AdminModelPriceExpressionPreviewResponse>()
  const [errorCode, setErrorCode] = useState<string>()
  const inputVersion = useRef(0)
  const request = useMemo(() => buildExpressionPreviewRequest(props.source, usage, ratios), [props.source, ratios, usage])
  const sourceVariables = expressionVariables(props.source)
  const sourceBytes = expressionSourceBytes(props.source)

  useEffect(() => {
    inputVersion.current += 1
    setResult(undefined)
    setErrorCode(undefined)
    reset()
  }, [props.source, reset])

  const updateUsage = (field: keyof ExpressionPreviewUsageDraft, value: string) => {
    inputVersion.current += 1
    setResult(undefined)
    setErrorCode(undefined)
    reset()
    setUsage((current) => ({ ...current, [field]: value }))
  }
  const updateRatio = (field: keyof ExpressionPreviewRatiosDraft, value: string) => {
    inputVersion.current += 1
    setResult(undefined)
    setErrorCode(undefined)
    reset()
    setRatios((current) => ({ ...current, [field]: value }))
  }
  const runPreview = async () => {
    if (!request) return
    const submittedVersion = inputVersion.current
    setErrorCode(undefined)
    try {
      const next = await mutateAsync(request)
      // 只接纳仍对应当前表单快照的响应，避免慢请求覆盖管理员后续输入。
      if (submittedVersion === inputVersion.current) setResult(next)
    } catch (caught) {
      if (submittedVersion === inputVersion.current) {
        setResult(undefined)
        setErrorCode(modelPriceErrorCode(caught) ?? 'unknown')
      }
    }
  }

  return (
    <section className="grid gap-4" aria-labelledby="model-price-expression-trial">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 id="model-price-expression-trial" className="text-xs font-semibold">{t('modelManagement.prices.expression.trial.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.expression.trial.description')}</p>
        </div>
        <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('modelManagement.prices.expression.trial.serverBadge')}</Chip>
      </div>

      <div className="grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-3">
        <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
          <span className="font-semibold">{t('modelManagement.prices.expression.previewSource')}</span>
          <span className="font-mono text-muted-foreground">{t('modelManagement.prices.expression.previewBytes', { count: sourceBytes })}</span>
        </div>
        <pre className="max-h-32 overflow-auto whitespace-pre-wrap break-all rounded-md bg-background px-3 py-2 font-mono text-[0.6875rem] leading-5">{props.source || t('modelManagement.prices.expression.previewEmpty')}</pre>
        <div className="flex flex-wrap items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
          <span>{t('modelManagement.prices.expression.previewVariables')}</span>
          {sourceVariables.length > 0 ? sourceVariables.map((variable) => <Chip key={variable} className="font-mono" size="sm" variant="flat">{variable}</Chip>) : <span>{t('modelManagement.prices.expression.previewNoVariables')}</span>}
        </div>
      </div>

      <div className="grid gap-3 border-t border-[var(--hairline)] pt-4">
        <div>
          <h4 className="text-xs font-semibold">{t('modelManagement.prices.expression.trial.usageTitle')}</h4>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.expression.trial.usageHint')}</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          {usageFields.map(([field, label, inputMode]) => (
            <ModelManagementField key={field} id={`expression-trial-${field}`} label={t(`modelManagement.prices.expression.trial.${label}`)} hint={t('modelManagement.prices.expression.trial.tokenUnit')}>
              <Input
                id={`expression-trial-${field}`}
                size="sm"
                value={usage[field]}
                onChange={(event) => updateUsage(field, event.target.value)}
                inputMode={inputMode}
                autoComplete="off"
                classNames={{ input: 'font-mono tabular-nums' }}
                aria-label={t(`modelManagement.prices.expression.trial.${label}`)}
              />
            </ModelManagementField>
          ))}
        </div>
        <div>
          <span className="mb-1.5 block text-[0.6875rem] font-medium text-muted-foreground">{t('modelManagement.prices.expression.trial.semantics')}</span>
          <div role="radiogroup" aria-label={t('modelManagement.prices.expression.trial.semantics')} className="grid grid-cols-2 gap-1 rounded-lg bg-surface-2 p-1">
            {(['inclusive', 'cache_separated'] as const).map((semantics: ExpressionPreviewSemantics) => (
              <Button
                key={semantics}
                type="button"
                size="sm"
                variant="light"
                role="radio"
                aria-checked={usage.semantics === semantics}
                className={usage.semantics === semantics ? 'bg-surface-1 hover:bg-surface-1' : undefined}
                onClick={() => updateUsage('semantics', semantics)}
              >
                {t(`modelManagement.prices.expression.trial.${semantics}`)}
              </Button>
            ))}
          </div>
        </div>
      </div>

      <div className="grid gap-3 border-t border-[var(--hairline)] pt-4">
        <div>
          <h4 className="text-xs font-semibold">{t('modelManagement.prices.expression.trial.ratiosTitle')}</h4>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.expression.trial.ratiosHint')}</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-3">
          {ratioFields.map(([field, label]) => (
            <ModelManagementField key={field} id={`expression-trial-ratio-${field}`} label={t(`modelManagement.prices.expression.trial.${label}`)} hint={t('modelManagement.prices.expression.trial.ratioUnit')}>
              <Input
                id={`expression-trial-ratio-${field}`}
                size="sm"
                value={ratios[field]}
                onChange={(event) => updateRatio(field, event.target.value)}
                inputMode="decimal"
                autoComplete="off"
                classNames={{ input: 'font-mono tabular-nums' }}
                aria-label={t(`modelManagement.prices.expression.trial.${label}`)}
              />
            </ModelManagementField>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--hairline)] pt-4">
        <p className="flex max-w-xl items-start gap-2 text-[0.6875rem] leading-4 text-muted-foreground">
          <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {t('modelManagement.prices.expression.trial.precisionHint')}
        </p>
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="bordered" onClick={props.onUseVisual}>
            <Check className="size-3.5" aria-hidden="true" />{t('modelManagement.prices.expression.previewBackToVisual')}
          </Button>
          <Button type="button" color="primary" size="sm" isDisabled={!request || isPending} onClick={() => void runPreview()}>
            {isPending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Calculator className="size-3.5" aria-hidden="true" />}
            {t(isPending ? 'modelManagement.prices.expression.trial.running' : 'modelManagement.prices.expression.trial.run')}
          </Button>
        </div>
      </div>

      {!request && props.source.trim() ? <p role="alert" className="text-xs text-destructive">{t('modelManagement.prices.expression.trial.invalidInput')}</p> : null}
      {errorCode ? <ExpressionPreviewError code={errorCode} /> : null}
      {result ? <ExpressionPreviewResult result={result} /> : null}
    </section>
  )
}

function ExpressionPreviewError({ code }: { code: string }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="flex items-start gap-2 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive">
      <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span>{t(`modelManagement.prices.errors.${code}`, { defaultValue: t('modelManagement.prices.errors.unknown') })}</span>
    </div>
  )
}

function ExpressionPreviewResult({ result }: { result: AdminModelPriceExpressionPreviewResponse }) {
  const { t } = useTranslation()
  const fields = expressionPreviewResultFields(result)
  const summary = [
    ['tier', result.matched_tier],
    ['base', `$${result.base_usd}`],
    ['total', `$${result.total_usd}`],
    ['quota', result.quota],
  ] as const
  return (
    <div role="status" aria-live="polite" className="grid gap-3 rounded-lg border border-success/20 bg-success/6 p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h4 className="text-xs font-semibold">{t('modelManagement.prices.expression.trial.resultTitle')}</h4>
          <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.prices.expression.trial.resultHint')}</p>
        </div>
        <Chip className="bg-success/12 text-success" size="sm" variant="flat">{t('modelManagement.prices.expression.trial.success')}</Chip>
      </div>
      <dl className="grid gap-2 sm:grid-cols-4">
        {summary.map(([key, value]) => (
          <div key={key} className="min-w-0 rounded-md border border-success/15 bg-background/35 px-2.5 py-2">
            <dt className="text-[0.6875rem] text-muted-foreground">{t(`modelManagement.prices.expression.trial.result.${key}`)}</dt>
            <dd className="mt-1 truncate font-mono text-xs tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      <div className="grid gap-2">
        <h5 className="text-[0.6875rem] font-semibold">{t('modelManagement.prices.expression.trial.normalizedVariables')}</h5>
        <div className="grid gap-1.5 sm:grid-cols-3">
          {fields.map(({ key, value }) => (
            <div key={key} className="flex items-center justify-between gap-2 rounded-md border border-[var(--hairline)] px-2.5 py-1.5 text-[0.6875rem]">
              <Chip className="font-mono" size="sm" variant="flat">{key}</Chip>
              <span className="font-mono tabular-nums">{value}</span>
            </div>
          ))}
        </div>
      </div>
      <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.expression.trial.resultServerSource')}</p>
    </div>
  )
}
