import { Button, Chip, Input, Switch, Textarea } from '@heroui/react'
import { useMemo, useState, type ReactNode } from 'react'
import { ArrowDownToLine, Braces, Code2, Eye, Info, RotateCcw, Sparkles } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminModelPriceSourceCandidate } from '@/lib/api/generated/types.gen'
import { ModelManagementField } from './model-management-field'
import {
  defaultExpressionVisualDraft,
  expressionFromVisualDraft,
  expressionPriceVariables,
  expressionVisualDraftIsValid,
  parseExpressionVisualDraft,
  type ExpressionVisualDraft,
} from './model-price-expression-model'
import { ModelPriceExpressionPreview } from './model-price-expression-preview'

type ExpressionEditorProps = {
  candidate?: AdminModelPriceSourceCandidate
  error?: string
  onChange: (value: string) => void
  value: string
}

type ExpressionEditorTab = 'visual' | 'raw' | 'preview'

/** 提供受限可视化构造、原始正文和提交前预览，服务端仍是最终校验权威。 */
export function ModelPriceExpressionEditor(props: ExpressionEditorProps) {
  const { t } = useTranslation()
  const parsed = useMemo(() => parseExpressionVisualDraft(props.value), [props.value])
  const [tab, setTab] = useState<ExpressionEditorTab>(parsed || !props.value ? 'visual' : 'raw')
  const [visualDraft, setVisualDraft] = useState<ExpressionVisualDraft>(() => parsed ?? defaultExpressionVisualDraft())
  const activeVisualDraft = parsed ?? visualDraft

  const updateVisual = (next: ExpressionVisualDraft) => {
    setVisualDraft(next)
    props.onChange(expressionFromVisualDraft(next))
  }
  const useVisualTemplate = () => {
    const next = defaultExpressionVisualDraft()
    updateVisual(next)
    setTab('visual')
  }
  const applyPreset = (preset: 'token' | 'cache') => {
    const current = activeVisualDraft
    const enabled = preset === 'token'
      ? { input: true, output: true, cacheRead: false, cacheCreation5m: false, cacheCreation1h: false }
      : { input: true, output: true, cacheRead: true, cacheCreation5m: true, cacheCreation1h: true }
    updateVisual({
      ...current,
      components: Object.fromEntries(
        expressionPriceVariables.map(({ key }) => [key, { ...current.components[key], enabled: enabled[key] }]),
      ) as ExpressionVisualDraft['components'],
    })
  }

  return (
    <section className="grid gap-3" aria-labelledby="model-price-expression">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h3 id="model-price-expression" className="text-xs font-semibold">{t('modelManagement.prices.expression.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.expression.description')}</p>
        </div>
        <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">v1</Chip>
      </div>

      <div role="tablist" aria-label={t('modelManagement.prices.expression.tabs.label')} className="grid grid-cols-3 gap-1 rounded-lg border border-[var(--hairline)] bg-surface-2/55 p-1">
        <ExpressionTab active={tab === 'visual'} icon={<Sparkles className="size-3.5" aria-hidden="true" />} label={t('modelManagement.prices.expression.tabs.visual')} onClick={() => setTab('visual')} />
        <ExpressionTab active={tab === 'raw'} icon={<Code2 className="size-3.5" aria-hidden="true" />} label={t('modelManagement.prices.expression.tabs.raw')} onClick={() => setTab('raw')} />
        <ExpressionTab active={tab === 'preview'} icon={<Eye className="size-3.5" aria-hidden="true" />} label={t('modelManagement.prices.expression.tabs.preview')} onClick={() => setTab('preview')} />
      </div>

      {tab === 'visual' ? (
        parsed || !props.value ? (
          <VisualEditor candidate={props.candidate} draft={activeVisualDraft} onChange={updateVisual} onPreset={applyPreset} />
        ) : (
          <div className="grid gap-3 rounded-lg border border-warning/20 bg-warning/8 p-3 text-xs leading-5 text-warning">
            <div className="flex items-start gap-2"><Info className="mt-0.5 size-4 shrink-0" aria-hidden="true" /><span>{t('modelManagement.prices.expression.visualUnavailable')}</span></div>
            <Button type="button" size="sm" variant="bordered" className="justify-self-start" onClick={useVisualTemplate}>
              <RotateCcw className="size-3.5" aria-hidden="true" />{t('modelManagement.prices.expression.useVisualTemplate')}
            </Button>
          </div>
        )
      ) : null}

      {tab === 'raw' ? (
        <div className="grid gap-2">
          <Textarea
            size="sm"
            value={props.value}
            onChange={(event) => props.onChange(event.target.value)}
            minRows={7}
            /* HeroUI 把 Textarea 的 spellCheck 收窄成字面量联合，用字符串字面量以符合类型。 */
            spellCheck="false"
            classNames={{ input: 'font-mono text-xs leading-5' }}
            isInvalid={props.error !== undefined}
            aria-label={t('modelManagement.prices.expression.rawLabel')}
          />
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.expression.rawHint')}</p>
        </div>
      ) : null}

      {tab === 'preview' ? (
        <ModelPriceExpressionPreview source={props.value} onUseVisual={parsed ? () => setTab('visual') : useVisualTemplate} />
      ) : null}

      {props.error ? <p role="alert" className="text-xs text-destructive">{props.error}</p> : null}
    </section>
  )
}

function ExpressionTab(props: { active: boolean; icon: ReactNode; label: string; onClick: () => void }) {
  return (
    <Button type="button" size="sm" variant="light" role="tab" aria-selected={props.active} className={props.active ? 'bg-surface-1 hover:bg-surface-1' : undefined} onClick={props.onClick}>
      {props.icon}{props.label}
    </Button>
  )
}

function VisualEditor(props: { candidate?: AdminModelPriceSourceCandidate; draft: ExpressionVisualDraft; onChange: (draft: ExpressionVisualDraft) => void; onPreset: (preset: 'token' | 'cache') => void }) {
  const { t } = useTranslation()
  const valid = expressionVisualDraftIsValid(props.draft)
  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" variant="bordered" onClick={() => props.onPreset('token')}>
          <Braces className="size-3.5" aria-hidden="true" />{t('modelManagement.prices.expression.presets.token')}
        </Button>
        <Button type="button" size="sm" variant="bordered" onClick={() => props.onPreset('cache')}>
          <Sparkles className="size-3.5" aria-hidden="true" />{t('modelManagement.prices.expression.presets.cache')}
        </Button>
      </div>
      <ModelManagementField id="expression-tier-name" label={t('modelManagement.prices.expression.tierName')} hint={t('modelManagement.prices.expression.tierNameHint')}>
        <Input
          id="expression-tier-name"
          size="sm"
          value={props.draft.tierName}
          onChange={(event) => props.onChange({ ...props.draft, tierName: event.target.value })}
          classNames={{ input: 'font-mono' }}
          autoComplete="off"
        />
      </ModelManagementField>
      <div className="grid gap-2">
        <div>
          <h4 className="text-xs font-semibold">{t('modelManagement.prices.expression.components')}</h4>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('modelManagement.prices.expression.componentsHint')}</p>
        </div>
        <div className="grid gap-2">
          {expressionPriceVariables.map(({ key, identifier, field }) => {
            const component = props.draft.components[key]
            const reference = key === 'input'
              ? props.candidate?.costs.input
              : key === 'output'
                ? props.candidate?.costs.output
                : key === 'cacheRead'
                  ? props.candidate?.costs.cache_read
                  : null
            return (
              <div key={key} className="grid grid-cols-[auto_1fr_auto] items-center gap-2 rounded-lg border border-[var(--hairline)] px-3 py-2">
                <Switch isSelected={component.enabled} size="sm" onValueChange={(enabled) => props.onChange({ ...props.draft, components: { ...props.draft.components, [key]: { ...component, enabled } } })} aria-label={t(`modelManagement.prices.fields.${field}`)} />
                <div className="min-w-0">
                  <div className="flex items-center gap-2"><Chip className="font-mono" size="sm" variant="flat">{identifier}</Chip><span className="truncate text-xs">{t(`modelManagement.prices.fields.${field}`)}</span></div>
                  <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.prices.expression.priceUnit')}</p>
                  {reference ? (
                    <Button type="button" size="sm" variant="light" className="mt-1 px-0" onClick={() => props.onChange({ ...props.draft, components: { ...props.draft.components, [key]: { enabled: true, rate: reference } } })}>
                      <ArrowDownToLine className="size-3.5" aria-hidden="true" />{t('modelManagement.prices.expression.useReference', { value: reference })}
                    </Button>
                  ) : null}
                </div>
                <Input
                  size="sm"
                  value={component.rate}
                  onChange={(event) => props.onChange({ ...props.draft, components: { ...props.draft.components, [key]: { ...component, rate: event.target.value } } })}
                  isDisabled={!component.enabled}
                  className="w-28"
                  classNames={{ input: 'font-mono tabular-nums' }}
                  inputMode="decimal"
                  aria-label={t('modelManagement.prices.expression.rate', { variable: identifier })}
                />
              </div>
            )
          })}
        </div>
      </div>
      <div className="flex items-start gap-2 rounded-lg border border-info/20 bg-info/8 px-3 py-2 text-xs leading-5 text-info">
        <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
        <span>{t('modelManagement.prices.expression.cacheBoundary')}</span>
      </div>
      {!valid ? <p role="alert" className="text-xs text-destructive">{t('modelManagement.prices.expression.visualInvalid')}</p> : null}
    </div>
  )
}
