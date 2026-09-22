import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Switch } from '@heroui/react'
import { CalendarClock, LoaderCircle, TicketPlus } from 'lucide-react'
import { useEffect, type ReactNode } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { IssuedAdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { redemptionErrorCode, useCreateAdminRedemptionBatch } from './redemption-api'
import {
  buildRedemptionBatchSchema,
  defaultRedemptionBatchValues,
  toRedemptionBatchRequest,
  type RedemptionBatchValues,
} from './redemption-form-model'

type RedemptionCreateSheetProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: IssuedAdminRedemptionBatch) => void
}

export function RedemptionCreateSheet({ open, onOpenChange, onIssued }: RedemptionCreateSheetProps) {
  const { t } = useTranslation()
  const mutation = useCreateAdminRedemptionBatch()
  const resetMutation = mutation.reset
  const form = useForm<RedemptionBatchValues>({
    defaultValues: defaultRedemptionBatchValues(),
    resolver: zodResolver(buildRedemptionBatchSchema({
      name: t('redemptions.validation.name'),
      quotaAmount: t('redemptions.validation.quotaAmount'),
      codeCount: t('redemptions.validation.codeCount'),
      expiresAt: t('redemptions.validation.expiresAt'),
    })),
  })
  const expires = form.watch('expires')
  const quotaAmount = form.watch('quotaAmount')
  const codeCount = form.watch('codeCount')
  const totalQuota = batchTotal(quotaAmount, codeCount)

  useEffect(() => {
    if (!open) return
    form.reset(defaultRedemptionBatchValues())
    resetMutation()
  }, [form, open, resetMutation])

  const submit = form.handleSubmit(async (values) => {
    const issued = await mutation.mutateAsync(toRedemptionBatchRequest(values))
    onIssued(issued)
    onOpenChange(false)
  })
  const errorKey = createErrorKey(redemptionErrorCode(mutation.error))

  return (
    <Drawer
      aria-describedby="redemption-create-description"
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
              <h2 className="text-base font-medium text-foreground">{t('redemptions.create.title')}</h2>
              <p className="text-sm text-muted-foreground" id="redemption-create-description">{t('redemptions.create.description')}</p>
            </DrawerHeader>
            <DrawerBody className="gap-0 p-0">
              <form className="grid gap-5 p-5" onSubmit={(event) => void submit(event)}>
              <div className="grid size-10 place-items-center rounded-lg bg-brand/10 text-brand">
                <TicketPlus className="size-4" aria-hidden="true" />
              </div>

              <RedemptionField
                id="redemption-name"
                label={t('redemptions.fields.name')}
                hint={t('redemptions.fields.nameHint')}
                error={form.formState.errors.name?.message}
              >
                <Input
                  id="redemption-name"
                  maxLength={80}
                  autoComplete="off"
                  isDisabled={mutation.isPending}
                  size="sm"
                  {...form.register('name')}
                />
              </RedemptionField>

              <div className="grid gap-4 sm:grid-cols-2">
                <RedemptionField
                  id="redemption-quota"
                  label={t('redemptions.fields.quotaAmount')}
                  hint={t('redemptions.fields.quotaAmountHint')}
                  error={form.formState.errors.quotaAmount?.message}
                >
                  <Input
                    id="redemption-quota"
                    inputMode="numeric"
                    autoComplete="off"
                    placeholder="100000"
                    isDisabled={mutation.isPending}
                    size="sm"
                    {...form.register('quotaAmount')}
                  />
                </RedemptionField>
                <RedemptionField
                  id="redemption-count"
                  label={t('redemptions.fields.codeCount')}
                  hint={t('redemptions.fields.codeCountHint')}
                  error={form.formState.errors.codeCount?.message}
                >
                  <Input
                    id="redemption-count"
                    type="number"
                    min={1}
                    max={1_000}
                    step={1}
                    isDisabled={mutation.isPending}
                    size="sm"
                    {...form.register('codeCount')}
                  />
                </RedemptionField>
              </div>

              <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 px-4 py-3">
                <div className="flex items-center justify-between gap-3">
                  <div className="min-w-0">
                    <label className="text-sm font-medium" htmlFor="redemption-expires">{t('redemptions.fields.expires')}</label>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('redemptions.fields.expiresHint')}</p>
                  </div>
                  <Controller
                    control={form.control}
                    name="expires"
                    render={({ field }) => (
                      <Switch
                        aria-label={t('redemptions.fields.expires')}
                        isDisabled={mutation.isPending}
                        isSelected={field.value}
                        size="sm"
                        onValueChange={field.onChange}
                      />
                    )}
                  />
                </div>
                {expires ? (
                  <div className="mt-4 border-t border-[var(--hairline)] pt-4">
                    <RedemptionField
                      id="redemption-expires-at"
                      label={t('redemptions.fields.expiresAt')}
                      error={form.formState.errors.expiresAt?.message}
                    >
                      <div className="relative">
                        <CalendarClock className="pointer-events-none absolute left-3 top-1/2 z-10 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                        <Input
                          id="redemption-expires-at"
                          type="datetime-local"
                          size="sm"
                          classNames={{ inputWrapper: 'pl-9' }}
                          isDisabled={mutation.isPending}
                          {...form.register('expiresAt')}
                        />
                      </div>
                    </RedemptionField>
                  </div>
                ) : null}
              </div>

              <div className="flex items-center justify-between gap-4 border-t border-[var(--hairline)] py-3 text-xs">
                <span className="text-muted-foreground">{t('redemptions.create.totalQuota')}</span>
                <span className="font-semibold tabular-nums">
                  {totalQuota ?? t('redemptions.create.totalPending')}
                </span>
              </div>

              {mutation.isError ? (
                <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                  {t(`redemptions.errors.${errorKey}`)}
                </p>
              ) : null}

              <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
                <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>
                  {t('redemptions.actions.cancel')}
                </Button>
                <Button type="submit" color="primary" isDisabled={mutation.isPending}>
                  {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <TicketPlus className="size-4" aria-hidden="true" />}
                  {t(mutation.isPending ? 'redemptions.actions.creating' : 'redemptions.actions.create')}
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

function RedemptionField({
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
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p role="alert" className="text-xs text-destructive">{error}</p> : hint ? <p className="text-xs leading-5 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

function batchTotal(quotaAmount: string, codeCount: string) {
  if (!/^[1-9]\d*$/.test(quotaAmount) || !/^[1-9]\d*$/.test(codeCount)) return undefined
  try {
    return (BigInt(quotaAmount) * BigInt(codeCount)).toLocaleString()
  } catch {
    return undefined
  }
}

function createErrorKey(code: string | undefined) {
  if (code === 'redemption_batch_conflict') return 'conflict'
  if (code === 'redemption_outcome_unknown') return 'outcomeUnknown'
  return 'create'
}
