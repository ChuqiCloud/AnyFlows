import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import type { StagedModelPriceDraft } from './model-price-form-model'

export type ModelPriceApplyItem = {
  model: string
  staged: StagedModelPriceDraft
}

type ModelPriceApplyDialogProps = {
  errorCode?: string
  items: ModelPriceApplyItem[]
  onApply: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
  pending: boolean
}

/** 在写入前逐项回显最终定价模式和冻结版本，避免来源证据被误认为正式价格。 */
export function ModelPriceApplyDialog(props: ModelPriceApplyDialogProps) {
  const { t } = useTranslation()
  return (
    <AlertDialog open={props.open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent className="max-w-3xl">
        <AlertDialogHeader>
          <AlertDialogTitle>{t('modelManagement.prices.apply.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('modelManagement.prices.apply.description', { count: props.items.length })}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="max-h-[55vh] space-y-2 overflow-y-auto pr-1">
          {props.items.map(({ model, staged }) => (
            <section key={model} className="rounded-lg border border-[var(--hairline)] p-3" aria-label={model}>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="min-w-0 truncate font-mono text-xs font-semibold">{model}</h3>
                <span className="text-[0.6875rem] text-muted-foreground">
                  {t(staged.expectedVersion === null ? 'modelManagement.prices.apply.createVersion' : 'modelManagement.prices.apply.updateVersion', { version: staged.expectedVersion })}
                </span>
              </div>
              <div className="mt-3 grid grid-cols-2 gap-x-3 gap-y-2 text-[0.6875rem] sm:grid-cols-5">
                {staged.contextWindow !== null ? (
                  <div className="col-span-2 sm:col-span-5">
                    <span className="text-muted-foreground">{t('modelManagement.prices.fields.contextWindow')}</span>{' '}
                    <span className="font-mono tabular-nums">{new Intl.NumberFormat().format(staged.contextWindow)} tokens</span>
                  </div>
                ) : null}
                {staged.draft.billingMode === 'expression' ? (
                  <div className="col-span-2 grid gap-1 sm:col-span-5">
                    <span className="text-muted-foreground">{t('modelManagement.prices.apply.expression')}</span>
                    <code className="break-all rounded bg-surface-2 px-2 py-1 font-mono text-[0.6875rem]">{staged.draft.expression}</code>
                  </div>
                ) : (
                  <>
                    <FinalPrice label={t('modelManagement.prices.fields.input')} value={staged.draft.input} />
                    <FinalPrice label={t('modelManagement.prices.fields.output')} value={staged.draft.output} />
                    <FinalPrice label={t('modelManagement.prices.fields.cacheRead')} value={staged.draft.cacheRead} />
                    <FinalPrice label={t('modelManagement.prices.fields.cacheCreation5m')} value={staged.draft.cacheCreation5m} />
                    <FinalPrice label={t('modelManagement.prices.fields.cacheCreation1h')} value={staged.draft.cacheCreation1h} />
                  </>
                )}
              </div>
            </section>
          ))}
        </div>
        <div className="rounded-lg bg-surface-2/55 px-3 py-2 text-xs leading-5 text-muted-foreground">
          {t('modelManagement.prices.apply.boundary')}
        </div>
        {props.errorCode ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(`modelManagement.prices.errors.${props.errorCode}`, { defaultValue: t('modelManagement.prices.errors.unknown') })}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={props.pending}>{t('modelManagement.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={props.pending || props.items.length === 0} onClick={(event) => { event.preventDefault(); props.onApply() }}>
            {props.pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t('modelManagement.prices.actions.apply', { count: props.items.length })}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function FinalPrice({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <span className="block truncate text-muted-foreground">{label}</span>
      <span className="mt-0.5 block truncate font-mono tabular-nums">${value}</span>
    </div>
  )
}
