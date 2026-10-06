import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Card, CardBody, CardHeader, Chip, Input, Select, SelectItem, Switch } from '@heroui/react'
import { ArrowUpRight, BellRing, Check, Clock3, Gauge, LoaderCircle, WalletCards } from 'lucide-react'
import { type ReactNode, useEffect } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { AdminBalanceAlertSettings } from '@/lib/api/generated/types.gen'
import { useUpdateAdminBalanceAlertSettings } from './billing-settings-api'
import {
  BALANCE_ALERT_WINDOW_OPTIONS,
  billingSettingsValues,
  buildBillingSettingsSchema,
  toBillingSettingsRequest,
  type BillingSettingsValues,
} from './billing-settings-form-model'

type BillingSettingsFormProps = {
  settings: AdminBalanceAlertSettings
}

/** 使用结构化控件编辑余额预警规则，并保留关闭状态下的配置草稿。 */
export function BillingSettingsForm({ settings }: BillingSettingsFormProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminBalanceAlertSettings()
  const form = useForm<BillingSettingsValues>({
    defaultValues: billingSettingsValues(settings),
    resolver: zodResolver(buildBillingSettingsSchema({
      invalidThreshold: t('billingSettings.validation.threshold'),
      invalidWindow: t('billingSettings.validation.window'),
      invalidSubscriptionPercent: t('billingSettings.validation.subscriptionPercent'),
    })),
  })

  useEffect(() => {
    form.reset(billingSettingsValues(settings))
  }, [form, settings])

  const enabled = form.watch('enabled')
  const subscriptionAlertEnabled = form.watch('subscriptionAlertEnabled')
  const anyEnabled = enabled || subscriptionAlertEnabled
  const currentWindow = form.watch('reminderIntervalSeconds')
  const usesCustomWindow = !BALANCE_ALERT_WINDOW_OPTIONS.some((seconds) => String(seconds) === currentWindow)
  // HeroUI Select 不接受空字符串 key，动态选项走 items + 渲染函数。
  const windowItems = [
    ...(usesCustomWindow
      ? [{ key: String(currentWindow), label: t('billingSettings.windows.custom', { seconds: currentWindow }) }]
      : []),
    ...BALANCE_ALERT_WINDOW_OPTIONS.map((seconds) => ({
      key: String(seconds),
      label: t(`billingSettings.windows.${seconds}`),
    })),
  ]
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toBillingSettingsRequest(values))
      form.reset(billingSettingsValues(saved))
    } catch {
      // 保留管理员输入，便于修正或在网络恢复后重试。
    }
  })

  return (
    <form className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_19rem]" onSubmit={onSubmit} noValidate>
      <Card className="overflow-hidden border border-[var(--hairline)]" shadow="none">
        <CardHeader className="flex-row items-start justify-between gap-3 border-b border-[var(--hairline)] bg-surface-1/45 p-5">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand">
              <BellRing className="size-4" aria-hidden="true" />
            </div>
            <div>
              <h3 className="text-[0.9375rem] leading-none font-semibold">{t('billingSettings.form.title')}</h3>
              <p className="mt-1 text-sm text-muted-foreground">{t('billingSettings.form.subtitle')}</p>
            </div>
          </div>
          <Chip className={anyEnabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'} size="sm" variant="flat">
            {t(anyEnabled ? 'billingSettings.status.enabled' : 'billingSettings.status.disabled')}
          </Chip>
        </CardHeader>
        <CardBody className="p-0">
          <SettingSectionHeader
            icon={<WalletCards className="size-4" aria-hidden="true" />}
            title={t('billingSettings.sections.wallet')}
            description={t('billingSettings.sections.walletHint')}
          />
          <SettingRow
            title={t('billingSettings.fields.enabled')}
            description={t('billingSettings.fields.enabledHint')}
            control={(
              <Controller
                control={form.control}
                name="enabled"
                render={({ field }) => (
                  <Switch
                    id="balance-alert-enabled"
                    aria-label={t('billingSettings.fields.enabled')}
                    isSelected={field.value}
                    size="sm"
                    onValueChange={field.onChange}
                  />
                )}
              />
            )}
          />
          <SettingRow
            title={t('billingSettings.fields.threshold')}
            description={t('billingSettings.fields.thresholdHint')}
            control={(
              <div className="w-full md:max-w-xs">
                <div className="relative">
                  <Input
                    className="pr-16 tabular-nums"
                    id="balance-alert-threshold"
                    inputMode="numeric"
                    isInvalid={!!form.formState.errors.defaultThreshold}
                    min={1}
                    size="sm"
                    step={1}
                    type="number"
                    {...form.register('defaultThreshold', { valueAsNumber: true })}
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
                    {t('billingSettings.fields.quotaUnit')}
                  </span>
                </div>
                {form.formState.errors.defaultThreshold ? (
                  <p className="mt-1.5 text-xs text-destructive">{form.formState.errors.defaultThreshold.message}</p>
                ) : null}
              </div>
            )}
          />
          <SettingRow
            title={t('billingSettings.fields.window')}
            description={t('billingSettings.fields.windowHint')}
            control={(
              <div className="w-full md:max-w-xs">
                <Controller
                  control={form.control}
                  name="reminderIntervalSeconds"
                  render={({ field }) => (
                    <Select
                      aria-label={t('billingSettings.fields.window')}
                      id="balance-alert-window"
                      isInvalid={!!form.formState.errors.reminderIntervalSeconds}
                      items={windowItems}
                      selectedKeys={field.value === '' ? [] : [field.value]}
                      size="sm"
                      onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
                    >
                      {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                    </Select>
                  )}
                />
                {form.formState.errors.reminderIntervalSeconds ? (
                  <p className="mt-1.5 text-xs text-destructive">{form.formState.errors.reminderIntervalSeconds.message}</p>
                ) : null}
              </div>
            )}
          />
          <SettingSectionHeader
            icon={<Gauge className="size-4" aria-hidden="true" />}
            title={t('billingSettings.sections.subscription')}
            description={t('billingSettings.sections.subscriptionHint')}
          />
          <SettingRow
            title={t('billingSettings.fields.subscriptionEnabled')}
            description={t('billingSettings.fields.subscriptionEnabledHint')}
            control={(
              <Controller
                control={form.control}
                name="subscriptionAlertEnabled"
                render={({ field }) => (
                  <Switch
                    id="subscription-alert-enabled"
                    aria-label={t('billingSettings.fields.subscriptionEnabled')}
                    isSelected={field.value}
                    size="sm"
                    onValueChange={field.onChange}
                  />
                )}
              />
            )}
          />
          <SettingRow
            title={t('billingSettings.fields.subscriptionPercent')}
            description={t('billingSettings.fields.subscriptionPercentHint')}
            control={(
              <div className="w-full md:max-w-xs">
                <div className="relative">
                  <Input
                    className="pr-12 tabular-nums"
                    id="subscription-alert-percent"
                    inputMode="numeric"
                    isInvalid={!!form.formState.errors.subscriptionRemainingPercent}
                    max={99}
                    min={1}
                    size="sm"
                    step={1}
                    type="number"
                    {...form.register('subscriptionRemainingPercent', { valueAsNumber: true })}
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
                    %
                  </span>
                </div>
                {form.formState.errors.subscriptionRemainingPercent ? (
                  <p className="mt-1.5 text-xs text-destructive">{form.formState.errors.subscriptionRemainingPercent.message}</p>
                ) : null}
              </div>
            )}
          />
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] px-5 py-4">
            <div className="text-xs text-muted-foreground">
              {mutation.isSuccess ? (
                <span className="flex items-center gap-1.5 text-success">
                  <Check className="size-3.5" aria-hidden="true" />
                  {t('billingSettings.status.saved')}
                </span>
              ) : mutation.isError ? (
                <span className="text-destructive">{t('billingSettings.errors.save')}</span>
              ) : (
                t('billingSettings.status.version', { version: settings.version })
              )}
            </div>
            <Button type="submit" color="primary" size="sm" isDisabled={!form.formState.isDirty || mutation.isPending}>
              {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
              {t('billingSettings.actions.save')}
            </Button>
          </div>
        </CardBody>
      </Card>

      <Card className="border border-[var(--hairline)]" shadow="none">
        <CardHeader className="flex flex-col gap-1.5 p-5">
          <div className="flex items-center gap-2 text-brand">
            <WalletCards className="size-4" aria-hidden="true" />
            <h3 className="text-[0.9375rem] leading-none font-semibold">{t('billingSettings.delivery.title')}</h3>
          </div>
          <p className="text-sm text-muted-foreground">{t('billingSettings.delivery.subtitle')}</p>
        </CardHeader>
        <CardBody className="grid gap-4 p-5 pt-0">
          <PolicyItem icon={<BellRing className="size-4" aria-hidden="true" />} label={t('billingSettings.delivery.dedup')} />
          <PolicyItem icon={<Clock3 className="size-4" aria-hidden="true" />} label={t('billingSettings.delivery.retry')} />
          <Button as="a" href="/console/system-settings/email" className="w-full justify-between" size="sm" type="button" variant="bordered">
            {t('billingSettings.actions.emailSettings')}
            <ArrowUpRight className="size-3.5" aria-hidden="true" />
          </Button>
        </CardBody>
      </Card>
    </form>
  )
}

function SettingRow({ title, description, control }: { title: string; description: string; control: ReactNode }) {
  return (
    <div className="grid gap-3 border-b border-[var(--hairline)] px-5 py-4 last:border-b-0 md:grid-cols-[minmax(0,1fr)_minmax(14rem,0.8fr)] md:items-center">
      <div>
        <p className="text-sm font-medium">{title}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      <div className="flex justify-start md:justify-end">{control}</div>
    </div>
  )
}

function SettingSectionHeader({ icon, title, description }: { icon: ReactNode; title: string; description: string }) {
  return (
    <div className="flex items-start gap-3 border-b border-[var(--hairline)] bg-surface-2/45 px-5 py-3">
      <span className="mt-0.5 shrink-0 text-brand">{icon}</span>
      <div>
        <p className="text-sm font-medium">{title}</p>
        <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
    </div>
  )
}

function PolicyItem({ icon, label }: { icon: ReactNode; label: string }) {
  return (
    <div className="flex items-start gap-3 text-xs leading-5 text-muted-foreground">
      <span className="mt-0.5 shrink-0 text-info">{icon}</span>
      <span>{label}</span>
    </div>
  )
}
