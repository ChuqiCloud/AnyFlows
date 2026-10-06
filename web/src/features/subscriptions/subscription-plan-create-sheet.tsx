import { zodResolver } from '@hookform/resolvers/zod'
import { CreditCard, Layers3, LoaderCircle } from 'lucide-react'
import { useEffect, type ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { subscriptionErrorCode, useCreateAdminSubscriptionPlan } from './subscription-api'
import {
  buildSubscriptionPlanSchema,
  defaultSubscriptionPlanValues,
  toSubscriptionPlanRequest,
  type SubscriptionPlanValues,
} from './subscription-form-model'

type SubscriptionPlanCreateSheetProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
}

/** 使用结构化名称、额度和周期创建不可变计划。 */
export function SubscriptionPlanCreateSheet({ open, onOpenChange }: SubscriptionPlanCreateSheetProps) {
  const { t } = useTranslation()
  const mutation = useCreateAdminSubscriptionPlan()
  const resetMutation = mutation.reset
  const form = useForm<SubscriptionPlanValues>({
    defaultValues: defaultSubscriptionPlanValues(),
    resolver: zodResolver(buildSubscriptionPlanSchema({
      name: t('subscriptions.validation.name'),
      quotaAmount: t('subscriptions.validation.quotaAmount'),
      priceCurrency: t('subscriptions.validation.priceCurrency'),
      priceAmountMinor: t('subscriptions.validation.priceAmountMinor'),
    })),
  })

  useEffect(() => {
    if (!open) return
    form.reset(defaultSubscriptionPlanValues())
    resetMutation()
  }, [form, open, resetMutation])

  const submit = form.handleSubmit(async (values) => {
    await mutation.mutateAsync(toSubscriptionPlanRequest(values))
    onOpenChange(false)
  })
  const errorKey = createErrorKey(subscriptionErrorCode(mutation.error))

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="subscription-plan-create-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('subscriptions.create.title')}</SheetTitle>
          <SheetDescription id="subscription-plan-create-description">
            {t('subscriptions.create.description')}
          </SheetDescription>
        </SheetHeader>
        <form className="grid gap-5 p-5" onSubmit={(event) => void submit(event)}>
          <span className="grid size-10 place-items-center rounded-lg bg-brand/10 text-brand">
            <Layers3 className="size-4" aria-hidden="true" />
          </span>

          <SubscriptionField
            id="subscription-plan-name"
            label={t('subscriptions.fields.name')}
            hint={t('subscriptions.fields.nameHint')}
            error={form.formState.errors.name?.message}
          >
            <Input
              id="subscription-plan-name"
              maxLength={80}
              autoComplete="off"
              disabled={mutation.isPending}
              {...form.register('name')}
            />
          </SubscriptionField>

          <div className="grid gap-4 border-y border-[var(--hairline)] py-4 sm:grid-cols-3">
            <SubscriptionField
              id="subscription-plan-provider"
              label={t('subscriptions.fields.priceProvider')}
              hint={t('subscriptions.fields.priceProviderHint')}
            >
              <Select id="subscription-plan-provider" disabled={mutation.isPending} {...form.register('priceProvider')}>
                <option value="stripe">Stripe</option>
                <option value="epay">EasyPay</option>
              </Select>
            </SubscriptionField>

            <SubscriptionField
              id="subscription-plan-currency"
              label={t('subscriptions.fields.priceCurrency')}
              hint={t('subscriptions.fields.priceCurrencyHint')}
              error={form.formState.errors.priceCurrency?.message}
            >
              <Input
                id="subscription-plan-currency"
                maxLength={3}
                autoComplete="off"
                disabled={mutation.isPending}
                {...form.register('priceCurrency', {
                  setValueAs: (value: string) => value.toUpperCase(),
                })}
              />
            </SubscriptionField>

            <SubscriptionField
              id="subscription-plan-price"
              label={t('subscriptions.fields.priceAmountMinor')}
              hint={t('subscriptions.fields.priceAmountMinorHint')}
              error={form.formState.errors.priceAmountMinor?.message}
            >
              <Input
                id="subscription-plan-price"
                inputMode="numeric"
                autoComplete="off"
                disabled={mutation.isPending}
                {...form.register('priceAmountMinor')}
              />
            </SubscriptionField>
          </div>

          <SubscriptionField
            id="subscription-plan-quota"
            label={t('subscriptions.fields.quotaAmount')}
            hint={t('subscriptions.fields.quotaAmountHint')}
            error={form.formState.errors.quotaAmount?.message}
          >
            <Input
              id="subscription-plan-quota"
              inputMode="numeric"
              autoComplete="off"
              placeholder="100000"
              disabled={mutation.isPending}
              {...form.register('quotaAmount')}
            />
          </SubscriptionField>

          <SubscriptionField
            id="subscription-plan-cycle"
            label={t('subscriptions.fields.cycle')}
            hint={t('subscriptions.fields.cycleHint')}
          >
            <Select id="subscription-plan-cycle" disabled={mutation.isPending} {...form.register('cycle')}>
              {(['daily', 'weekly', 'monthly', 'yearly'] as const).map((cycle) => (
                <option key={cycle} value={cycle}>{t(`subscriptions.cycle.${cycle}`)}</option>
              ))}
            </Select>
          </SubscriptionField>

          <div className="flex items-center gap-3 border-y border-[var(--hairline)] py-3 text-xs text-muted-foreground">
            <CreditCard className="size-4 shrink-0 text-brand" aria-hidden="true" />
            <span>{t('subscriptions.create.snapshotNotice')}</span>
          </div>

          {mutation.isError ? (
            <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              {t(`subscriptions.errors.${errorKey}`)}
            </p>
          ) : null}

          <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button type="button" variant="secondary" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>
              {t('subscriptions.actions.cancel')}
            </Button>
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Layers3 aria-hidden="true" />}
              {t(mutation.isPending ? 'subscriptions.actions.creatingPlan' : 'subscriptions.actions.createPlan')}
            </Button>
          </div>
        </form>
      </SheetContent>
    </Sheet>
  )
}

function SubscriptionField({
  id,
  label,
  hint,
  error,
  children,
}: {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-2">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error
        ? <p role="alert" className="text-xs text-destructive">{error}</p>
        : hint ? <p className="text-xs leading-5 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

function createErrorKey(code: string | undefined) {
  if (code === 'subscription_conflict') return 'createConflict'
  if (code === 'subscription_outcome_unknown') return 'outcomeUnknown'
  return 'createPlan'
}
