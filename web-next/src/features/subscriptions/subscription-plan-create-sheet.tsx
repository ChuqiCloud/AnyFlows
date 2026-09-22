import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Select, SelectItem } from '@heroui/react'
import { CreditCard, Layers3, LoaderCircle } from 'lucide-react'
import { useEffect, type ReactNode } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

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

/** 计划价格提供方为固定枚举，静态选项即可满足。 */
const PROVIDER_ITEMS = [
  { key: 'stripe', label: 'Stripe' },
  { key: 'epay', label: 'EasyPay' },
]

const CYCLE_KEYS = ['daily', 'weekly', 'monthly', 'yearly'] as const

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
  const cycleItems = CYCLE_KEYS.map((cycle) => ({ key: cycle, label: t(`subscriptions.cycle.${cycle}`) }))

  return (
    <Drawer
      aria-describedby="subscription-plan-create-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('subscriptions.create.title')}</h2>
              <p className="text-sm text-muted-foreground" id="subscription-plan-create-description">
                {t('subscriptions.create.description')}
              </p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-0">
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
              autoComplete="off"
              id="subscription-plan-name"
              isDisabled={mutation.isPending}
              maxLength={80}
              size="sm"
              {...form.register('name')}
            />
          </SubscriptionField>

          <div className="grid gap-4 border-t border-[var(--hairline)] py-4 sm:grid-cols-3">
            <SubscriptionField
              id="subscription-plan-provider"
              label={t('subscriptions.fields.priceProvider')}
              hint={t('subscriptions.fields.priceProviderHint')}
            >
              <Controller
                control={form.control}
                name="priceProvider"
                render={({ field }) => (
                  <Select
                    aria-label={t('subscriptions.fields.priceProvider')}
                    id="subscription-plan-provider"
                    isDisabled={mutation.isPending}
                    items={PROVIDER_ITEMS}
                    selectedKeys={[field.value]}
                    size="sm"
                    onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'stripe'))}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              />
            </SubscriptionField>

            <SubscriptionField
              id="subscription-plan-currency"
              label={t('subscriptions.fields.priceCurrency')}
              hint={t('subscriptions.fields.priceCurrencyHint')}
              error={form.formState.errors.priceCurrency?.message}
            >
              <Input
                autoComplete="off"
                id="subscription-plan-currency"
                isDisabled={mutation.isPending}
                maxLength={3}
                size="sm"
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
                autoComplete="off"
                id="subscription-plan-price"
                inputMode="numeric"
                isDisabled={mutation.isPending}
                size="sm"
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
              autoComplete="off"
              id="subscription-plan-quota"
              inputMode="numeric"
              isDisabled={mutation.isPending}
              placeholder="100000"
              size="sm"
              {...form.register('quotaAmount')}
            />
          </SubscriptionField>

          <SubscriptionField
            id="subscription-plan-cycle"
            label={t('subscriptions.fields.cycle')}
            hint={t('subscriptions.fields.cycleHint')}
          >
            <Controller
              control={form.control}
              name="cycle"
              render={({ field }) => (
                <Select
                  aria-label={t('subscriptions.fields.cycle')}
                  id="subscription-plan-cycle"
                  isDisabled={mutation.isPending}
                  items={cycleItems}
                  selectedKeys={[field.value]}
                  size="sm"
                  onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'monthly'))}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              )}
            />
          </SubscriptionField>

          <div className="flex items-center gap-3 border-t border-[var(--hairline)] py-3 text-xs text-muted-foreground">
            <CreditCard className="size-4 shrink-0 text-brand" aria-hidden="true" />
            <span>{t('subscriptions.create.snapshotNotice')}</span>
          </div>

          {mutation.isError ? (
            <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              {t(`subscriptions.errors.${errorKey}`)}
            </p>
          ) : null}

          <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>
              {t('subscriptions.actions.cancel')}
            </Button>
            <Button color="primary" isDisabled={mutation.isPending} type="submit">
              {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Layers3 className="size-4" aria-hidden="true" />}
              {t(mutation.isPending ? 'subscriptions.actions.creatingPlan' : 'subscriptions.actions.createPlan')}
            </Button>
          </div>
              </form>
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
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
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
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
