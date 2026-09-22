import { Button, Checkbox, Chip } from '@heroui/react'
import { providerDisplayName } from '@/components/brand/provider-catalog'

import { CircleDollarSign, Pencil, SearchX } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ModelPriceValue } from '@/features/models/model-price-value'
import type {
  AdminModel,
  AdminModelPrice,
  AdminModelPriceSourceCandidate,
  AdminModelPriceValues,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'
import {
  modelPriceCandidateNeedsReview,
  modelPriceDraftIsComplete,
  type StagedModelPriceDraft,
} from './model-price-form-model'
import type { ModelPriceSource } from './model-price-api'

export type ModelPriceRow = {
  candidate?: AdminModelPriceSourceCandidate
  model: AdminModel
  price?: AdminModelPrice
  source?: ModelPriceSource
  staged?: StagedModelPriceDraft
}

type ModelPriceTableProps = {
  onEdit: (row: ModelPriceRow) => void
  onSelect: (model: string, selected: boolean) => void
  priceIndexComplete: boolean
  rows: ModelPriceRow[]
  selected: Set<string>
}

/** 同时呈现正式价格、待提交草稿和公开来源复核状态。 */
export function ModelPriceTable(props: ModelPriceTableProps) {
  const { t } = useTranslation()
  if (props.rows.length === 0) {
    return (
      <div className="grid min-h-52 place-items-center rounded-xl border border-dashed border-[var(--hairline)] px-6 text-center">
        <div>
          <SearchX className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t('modelManagement.prices.empty.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.prices.empty.body')}</p>
        </div>
      </div>
    )
  }
  return (
    <>
      <div className="hidden overflow-x-auto rounded-xl border border-[var(--hairline)] md:block">
        <table className="w-full min-w-[860px] border-collapse text-left text-xs">
          <thead className="bg-surface-2/65 text-muted-foreground">
            <tr>
              <th className="w-10 px-3 py-2"><span className="sr-only">{t('modelManagement.prices.columns.select')}</span></th>
              <th className="w-[29%] px-3 py-2 font-medium">{t('modelManagement.prices.columns.model')}</th>
              <th className="w-[19%] px-3 py-2 font-medium">{t('modelManagement.prices.columns.status')}</th>
              <th className="w-[13%] px-3 py-2 font-medium">{t('modelManagement.prices.fields.input')}</th>
              <th className="w-[13%] px-3 py-2 font-medium">{t('modelManagement.prices.fields.output')}</th>
              <th className="w-[13%] px-3 py-2 font-medium">{t('modelManagement.prices.fields.cacheRead')}</th>
              <th className="w-[13%] px-3 py-2 text-right"><span className="sr-only">{t('modelManagement.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {props.rows.map((row) => {
              const display = displayPrice(row)
              return (
                <tr key={row.model.model} className="align-middle hover:bg-surface-2/35">
                  <td className="px-3 py-3"><RowCheckbox row={row} selected={props.selected.has(row.model.model)} onSelect={props.onSelect} /></td>
                  <td className="px-3 py-3"><ModelIdentity row={row} priceIndexComplete={props.priceIndexComplete} /></td>
                  <td className="px-3 py-3"><PriceStatus row={row} priceIndexComplete={props.priceIndexComplete} /></td>
                  <td className="px-3 py-3"><ModelPriceValue label={t('modelManagement.prices.fields.input')} value={display.values?.input} /></td>
                  <td className="px-3 py-3"><ModelPriceValue label={t('modelManagement.prices.fields.output')} value={display.values?.output} /></td>
                  <td className="px-3 py-3"><ModelPriceValue label={t('modelManagement.prices.fields.cacheRead')} value={display.values?.cache_read} /></td>
                  <td className="px-3 py-3 text-right"><EditButton row={row} onEdit={props.onEdit} /></td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {props.rows.map((row) => {
          const display = displayPrice(row)
          return (
            <article key={row.model.model} className="min-w-0 rounded-xl border border-[var(--hairline)] p-3">
              <div className="flex items-start gap-3">
                <RowCheckbox row={row} selected={props.selected.has(row.model.model)} onSelect={props.onSelect} />
                <div className="min-w-0 flex-1">
                  <ModelIdentity row={row} priceIndexComplete={props.priceIndexComplete} />
                  <div className="mt-2"><PriceStatus row={row} priceIndexComplete={props.priceIndexComplete} /></div>
                </div>
                <EditButton row={row} onEdit={props.onEdit} />
              </div>
              <div className="mt-3 grid grid-cols-3 gap-2 border-t border-[var(--hairline)] pt-3">
                <ModelPriceValue label={t('modelManagement.prices.fields.input')} value={display.values?.input} />
                <ModelPriceValue label={t('modelManagement.prices.fields.output')} value={display.values?.output} />
                <ModelPriceValue label={t('modelManagement.prices.fields.cacheRead')} value={display.values?.cache_read} />
              </div>
            </article>
          )
        })}
      </div>
    </>
  )
}

function ModelIdentity({ row, priceIndexComplete }: { row: ModelPriceRow; priceIndexComplete: boolean }) {
  const { t } = useTranslation()
  const missingPublicPrice = priceIndexComplete
    && !row.price
    && row.model.visibility === 'public'
    && row.model.lifecycle === 'active'
  return (
    <div className="min-w-0">
      <div className="truncate font-medium">{row.model.display_name}</div>
      <div className="mt-0.5 truncate font-mono text-[0.6875rem] text-muted-foreground">{row.model.model}</div>
      <div className="mt-1 flex flex-wrap items-center gap-1.5">
        <Chip size="sm" variant="flat">{providerDisplayName(row.model.provider)}</Chip>
        {row.candidate && row.source ? <Chip size="sm" variant="flat">{t(`modelManagement.prices.sources.${row.source}`)}</Chip> : null}
      </div>
      {missingPublicPrice ? <p className="mt-1.5 text-[0.6875rem] leading-4 text-warning">{t('modelManagement.prices.status.publicMissing')}</p> : null}
    </div>
  )
}

function PriceStatus({ row, priceIndexComplete }: { row: ModelPriceRow; priceIndexComplete: boolean }) {
  const { t } = useTranslation()
  const status = row.staged
    ? 'staged'
    : row.price?.billing_mode === 'expression'
      ? 'expression'
      : row.price?.billing_mode === 'free'
      ? 'free'
      : row.price
        ? 'priced'
        : priceIndexComplete
          ? 'unpriced'
          : 'checking'
  const styles: Record<typeof status, string> = {
    staged: 'bg-warning/10 text-warning',
    free: 'bg-success/10 text-success',
    expression: 'bg-warning/10 text-warning',
    priced: 'bg-info/10 text-info',
    unpriced: 'bg-destructive/10 text-destructive',
    checking: 'text-muted-foreground',
  }
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <Chip className={cn(styles[status])} size="sm" variant="flat">{t(`modelManagement.prices.status.${status}`)}</Chip>
      {row.staged?.draft.billingMode === 'expression' ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('modelManagement.prices.status.expression')}</Chip> : null}
      {modelPriceCandidateNeedsReview(row.candidate) ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('modelManagement.prices.status.review')}</Chip> : null}
      {row.staged && row.price && row.staged.expectedVersion !== row.price.version ? <Chip className="bg-destructive/10 text-destructive" size="sm" variant="flat">{t('modelManagement.prices.status.versionChanged')}</Chip> : null}
      {row.price ? <span className="font-mono text-[0.6875rem] text-muted-foreground">v{row.price.version}</span> : null}
    </div>
  )
}

function RowCheckbox(props: {
  onSelect: (model: string, selected: boolean) => void
  row: ModelPriceRow
  selected: boolean
}) {
  const { t } = useTranslation()
  const versionReady = !props.row.price || props.row.staged?.expectedVersion === props.row.price.version
  const enabled = versionReady
    && modelPriceDraftIsComplete(props.row.staged?.draft)
  const id = `select-price-${props.row.model.id}`
  return (
    <SiteTooltip content={t(enabled ? 'modelManagement.prices.actions.selectionReady' : 'modelManagement.prices.actions.editBeforeSelect')}>
      <span className="inline-flex">
        <Checkbox
          id={id}
          isSelected={props.selected}
          isDisabled={!enabled}
          size="sm"
          aria-label={t('modelManagement.prices.actions.select', { model: props.row.model.model })}
          onValueChange={(checked) => props.onSelect(props.row.model.model, checked === true)}
        />
      </span>
    </SiteTooltip>
  )
}

function EditButton({ row, onEdit }: { row: ModelPriceRow; onEdit: (row: ModelPriceRow) => void }) {
  const { t } = useTranslation()
  return (
    <SiteTooltip content={t(row.price?.billing_mode === 'expression' ? 'modelManagement.prices.actions.expressionReadOnly' : row.price || row.staged ? 'modelManagement.prices.actions.editPrice' : 'modelManagement.prices.actions.setPrice')}>
      <span className="inline-flex">
        <Button isIconOnly type="button" size="sm" variant="light" className="size-10 md:size-8" aria-label={t('modelManagement.prices.actions.edit', { model: row.model.model })} onClick={() => onEdit(row)}>
          {row.price || row.staged ? <Pencil className="size-3.5" aria-hidden="true" /> : <CircleDollarSign className="size-3.5" aria-hidden="true" />}
        </Button>
      </span>
    </SiteTooltip>
  )
}

function displayPrice(row: ModelPriceRow): { values?: AdminModelPriceValues } {
  if (!row.staged) return row.price?.billing_mode === 'expression' ? {} : { values: row.price?.prices }
  const draft = row.staged.draft
  if (draft.billingMode === 'expression') return {}
  return {
    values: {
      input: draft.input,
      output: draft.output,
      cache_read: draft.cacheRead,
      cache_creation_5m: draft.cacheCreation5m,
      cache_creation_1h: draft.cacheCreation1h,
    },
  }
}
