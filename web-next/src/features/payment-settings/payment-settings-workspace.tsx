import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Checkbox, Chip, Input, Switch } from '@heroui/react'
import { Check, Copy, CreditCard, KeyRound, Landmark, LoaderCircle, Settings2, ShieldCheck, TriangleAlert } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { Controller, useForm, type FieldError, type UseFormReturn } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { useUpdateAdminPaymentSettings } from './payment-settings-api'
import {
  buildPaymentSettingsSchema,
  paymentCallbackUrls,
  paymentSettingsValues,
  toPaymentSettingsRequest,
  type PaymentSettingsValues,
} from './payment-settings-form-model'
import type { AdminPaymentSettings } from './payment-settings-types'

type PaymentSettingsWorkspaceProps = {
  publicBaseUrl?: string | null
  settings: AdminPaymentSettings
}

/** 集中管理支付 Provider、脱敏密钥更新和固定回调入口。 */
export function PaymentSettingsWorkspace({ publicBaseUrl, settings }: PaymentSettingsWorkspaceProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminPaymentSettings()
  const form = useForm<PaymentSettingsValues>({
    defaultValues: paymentSettingsValues(settings),
    resolver: zodResolver(buildPaymentSettingsSchema(settings, {
      epayGatewayUrl: t('paymentSettings.validation.epayGatewayUrl'),
      epayMerchantId: t('paymentSettings.validation.epayMerchantId'),
      epayMerchantKey: t('paymentSettings.validation.epayMerchantKey'),
      epayPaymentMethod: t('paymentSettings.validation.epayPaymentMethod'),
      epayRefund: t('paymentSettings.validation.epayRefund'),
      epayPublicBaseUrl: t('paymentSettings.validation.epayPublicBaseUrl'),
      epayQuota: t('paymentSettings.validation.epayQuota'),
      stripePublishableKey: t('paymentSettings.validation.stripePublishableKey'),
      stripeSecretKey: t('paymentSettings.validation.stripeSecretKey'),
      stripeTolerance: t('paymentSettings.validation.stripeTolerance'),
      stripeWebhookSecret: t('paymentSettings.validation.stripeWebhookSecret'),
    }, publicBaseUrl)),
  })

  useEffect(() => {
    form.reset(paymentSettingsValues(settings))
  }, [form, settings])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toPaymentSettingsRequest(values, settings.version))
      form.reset(paymentSettingsValues(saved))
    } catch {
      // 保留管理员输入，错误只展示固定文案，避免泄露支付服务响应。
    }
  })
  const epayNeedsPublicBaseUrl = form.watch('epayEnabled') && !publicBaseUrl

  return (
    <form className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_20rem]" onSubmit={onSubmit} noValidate>
      <div className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55">
        <ProviderSection
          icon={<CreditCard className="size-4" aria-hidden="true" />}
          title={t('paymentSettings.stripe.title')}
          description={t('paymentSettings.stripe.description')}
          enabled={form.watch('stripeEnabled')}
          switchId="payment-stripe-enabled"
          onEnabledChange={(checked) => form.setValue('stripeEnabled', checked, { shouldDirty: true, shouldValidate: true })}
          enabledLabel={t('paymentSettings.fields.stripeEnabled')}
          statusLabel={t(form.watch('stripeEnabled') ? 'paymentSettings.status.enabled' : 'paymentSettings.status.disabled')}
        >
          <TextField form={form} name="stripePublishableKey" id="payment-stripe-publishable" label={t('paymentSettings.fields.stripePublishableKey')} placeholder="pk_live_..." />
          <SecretField form={form} name="stripeSecretKey" clearName="clearStripeSecretKey" configured={settings.stripe_secret_key_configured} id="payment-stripe-secret" label={t('paymentSettings.fields.stripeSecretKey')} />
          <SecretField form={form} name="stripeWebhookSecret" clearName="clearStripeWebhookSecret" configured={settings.stripe_webhook_secret_configured} id="payment-stripe-webhook-secret" label={t('paymentSettings.fields.stripeWebhookSecret')} />
          <NumberField form={form} name="stripeSignatureToleranceSeconds" id="payment-stripe-tolerance" label={t('paymentSettings.fields.stripeTolerance')} suffix={t('paymentSettings.units.seconds')} />
        </ProviderSection>

        <ProviderSection
          icon={<Landmark className="size-4" aria-hidden="true" />}
          title={t('paymentSettings.epay.title')}
          description={t('paymentSettings.epay.description')}
          enabled={form.watch('epayEnabled')}
          switchId="payment-epay-enabled"
          onEnabledChange={(checked) => {
            form.setValue('epayEnabled', checked, { shouldDirty: true, shouldValidate: true })
            if (!checked) form.setValue('epayRefundEnabled', false, { shouldDirty: true, shouldValidate: true })
          }}
          enabledLabel={t('paymentSettings.fields.epayEnabled')}
          statusLabel={t(form.watch('epayEnabled') ? 'paymentSettings.status.enabled' : 'paymentSettings.status.disabled')}
          last
        >
          {epayNeedsPublicBaseUrl ? (
            <div role="alert" className="flex flex-wrap items-start gap-2.5 rounded-lg border border-warning/25 bg-warning/8 p-3 sm:col-span-2 sm:flex-nowrap">
              <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
              <div className="min-w-0 flex-1">
                <p className="text-xs font-medium">{t('paymentSettings.epay.publicBaseUrlRequired')}</p>
                <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('paymentSettings.epay.publicBaseUrlHint')}</p>
              </div>
              <Button as="a" href="/console/system-settings/site" size="sm" type="button" variant="bordered">
                <Settings2 className="size-3.5" aria-hidden="true" />{t('paymentSettings.actions.configureSite')}
              </Button>
            </div>
          ) : null}
          <TextField form={form} name="epayGatewayUrl" id="payment-epay-gateway" label={t('paymentSettings.fields.epayGatewayUrl')} placeholder="https://pay.example.com" />
          <TextField form={form} name="epayMerchantId" id="payment-epay-merchant" label={t('paymentSettings.fields.epayMerchantId')} />
          <SecretField form={form} name="epayMerchantKey" clearName="clearEpayMerchantKey" configured={settings.epay_merchant_key_configured} id="payment-epay-key" label={t('paymentSettings.fields.epayMerchantKey')} />
          <NumberField form={form} name="epayQuotaPerCny" id="payment-epay-quota" label={t('paymentSettings.fields.epayQuotaPerCny')} suffix={t('paymentSettings.units.quota')} />
          <div className="grid gap-2 sm:grid-cols-2">
            <ToggleField form={form} name="epayAlipayEnabled" id="payment-epay-alipay" label={t('paymentSettings.fields.alipay')} />
            <ToggleField form={form} name="epayWxpayEnabled" id="payment-epay-wxpay" label={t('paymentSettings.fields.wxpay')} />
            <ToggleField form={form} name="epayQrEnabled" id="payment-epay-qr" label={t('paymentSettings.fields.epayQrEnabled')} disabled={!form.watch('epayEnabled')} />
            <ToggleField
              form={form}
              name="epayRefundEnabled"
              id="payment-epay-refund"
              label={t('paymentSettings.fields.epayRefundEnabled')}
              disabled={!form.watch('epayEnabled')}
              onCheckedChange={(checked) => {
                if (!checked) form.setValue('refundAutoSubmitEnabled', false, { shouldDirty: true, shouldValidate: true })
              }}
            />
            <ToggleField
              form={form}
              name="refundAutoSubmitEnabled"
              id="payment-refund-auto-submit"
              label={t('paymentSettings.fields.refundAutoSubmitEnabled')}
              disabled={!form.watch('epayRefundEnabled')}
            />
          </div>
          <p className="text-[0.6875rem] leading-4 text-muted-foreground sm:col-span-2">{t('paymentSettings.epay.refundHint')}</p>
        </ProviderSection>

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-5 py-3">
          <div className="text-xs">
            {mutation.isError ? <span role="alert" className="text-destructive">{t('paymentSettings.errors.save')}</span> : mutation.isSuccess && !form.formState.isDirty ? (
              <span className="inline-flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('paymentSettings.state.saved')}</span>
            ) : form.formState.isDirty ? <span className="text-muted-foreground">{t('paymentSettings.state.unsaved')}</span> : <span className="text-muted-foreground">{t('paymentSettings.state.synced', { version: settings.version })}</span>}
          </div>
          <Button type="submit" color="primary" isDisabled={!form.formState.isDirty || mutation.isPending || epayNeedsPublicBaseUrl}>
            {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'paymentSettings.actions.saving' : 'paymentSettings.actions.save')}
          </Button>
        </div>
      </div>

      <aside className="grid gap-4">
        <CallbackPanel publicBaseUrl={publicBaseUrl} />
        <section className="rounded-xl border border-info/20 bg-info/8 p-4">
          <div className="flex items-start gap-2.5">
            <ShieldCheck className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
            <div>
              <h3 className="text-sm font-semibold">{t('paymentSettings.security.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('paymentSettings.security.description')}</p>
            </div>
          </div>
        </section>
      </aside>
    </form>
  )
}

function ProviderSection(props: {
  children: ReactNode
  description: string
  enabled: boolean
  enabledLabel: string
  icon: ReactNode
  last?: boolean
  onEnabledChange: (checked: boolean) => void
  statusLabel: string
  switchId: string
  title: string
}) {
  return (
    <section className={props.last ? undefined : 'border-b border-[var(--hairline)]'}>
      <div className="flex flex-col gap-3 border-b border-[var(--hairline)] bg-surface-2/20 px-5 py-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand">{props.icon}</span>
          <div>
            <div className="flex flex-wrap items-center gap-2"><h3 className="text-sm font-semibold">{props.title}</h3><Chip className={props.enabled ? 'border-success/25 bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'} size="sm" variant="flat">{props.statusLabel}</Chip></div>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{props.description}</p>
          </div>
        </div>
        <div className="flex items-center gap-2"><label className="text-xs" htmlFor={props.switchId}>{props.enabledLabel}</label><Switch id={props.switchId} isSelected={props.enabled} size="sm" onValueChange={props.onEnabledChange} /></div>
      </div>
      <div className="grid gap-4 p-5 sm:grid-cols-2">{props.children}</div>
    </section>
  )
}

type TextName = 'epayGatewayUrl' | 'epayMerchantId' | 'stripePublishableKey'
function TextField({ form, id, label, name, placeholder }: { form: UseFormReturn<PaymentSettingsValues>; id: string; label: string; name: TextName; placeholder?: string }) {
  const error = form.formState.errors[name] as FieldError | undefined
  return <div className="grid gap-1.5"><label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label><Input autoComplete="off" id={id} isInvalid={!!error} placeholder={placeholder} size="sm" {...form.register(name)} />{error ? <p className="text-xs text-destructive">{error.message}</p> : null}</div>
}

type SecretName = 'epayMerchantKey' | 'stripeSecretKey' | 'stripeWebhookSecret'
type ClearName = 'clearEpayMerchantKey' | 'clearStripeSecretKey' | 'clearStripeWebhookSecret'
function SecretField({ clearName, configured, form, id, label, name }: { clearName: ClearName; configured: boolean; form: UseFormReturn<PaymentSettingsValues>; id: string; label: string; name: SecretName }) {
  const { t } = useTranslation()
  const clear = form.watch(clearName)
  const error = form.formState.errors[name] as FieldError | undefined
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label><Chip className={configured ? 'border-success/25 bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'} size="sm" startContent={<KeyRound className="size-3.5" aria-hidden="true" />} variant="flat">{t(configured ? 'paymentSettings.secrets.configured' : 'paymentSettings.secrets.missing')}</Chip></div>
      <Input autoComplete="new-password" id={id} isDisabled={clear} isInvalid={!!error} placeholder={configured ? t('paymentSettings.secrets.replacePlaceholder') : t('paymentSettings.secrets.createPlaceholder')} size="sm" type="password" {...form.register(name)} />
      {configured ? <label className="flex items-center gap-2 text-[0.6875rem] text-muted-foreground"><Controller control={form.control} name={clearName} render={({ field }) => <Checkbox isSelected={field.value} size="sm" onValueChange={(checked) => { field.onChange(checked === true); if (checked) form.setValue(name, '', { shouldDirty: true, shouldValidate: true }) }} />} />{t('paymentSettings.secrets.clear')}</label> : <p className="text-[0.6875rem] text-muted-foreground">{t('paymentSettings.secrets.neverShown')}</p>}
      {error ? <p className="text-xs text-destructive">{error.message}</p> : null}
    </div>
  )
}

type NumberName = 'epayQuotaPerCny' | 'stripeSignatureToleranceSeconds'
function NumberField({ form, id, label, name, suffix }: { form: UseFormReturn<PaymentSettingsValues>; id: string; label: string; name: NumberName; suffix: string }) {
  const error = form.formState.errors[name] as FieldError | undefined
  return <div className="grid gap-1.5"><label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label><div className="relative"><Input className="tabular-nums" id={id} isInvalid={!!error} min={1} size="sm" type="number" {...form.register(name, { valueAsNumber: true })} /><span data-input-suffix className="pointer-events-none absolute right-3 top-0 grid h-8 place-items-center pr-16 text-xs leading-none text-muted-foreground">{suffix}</span></div>{error ? <p className="text-xs text-destructive">{error.message}</p> : null}</div>
}

type ToggleName = 'epayAlipayEnabled' | 'epayQrEnabled' | 'epayRefundEnabled' | 'epayWxpayEnabled' | 'refundAutoSubmitEnabled'
function ToggleField({ disabled, form, id, label, name, onCheckedChange }: { disabled?: boolean; form: UseFormReturn<PaymentSettingsValues>; id: string; label: string; name: ToggleName; onCheckedChange?: (checked: boolean) => void }) {
  return <div className="flex items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2.5"><label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label><Controller control={form.control} name={name} render={({ field }) => <Switch id={id} isDisabled={disabled} isSelected={field.value} size="sm" onValueChange={(checked) => { field.onChange(checked); onCheckedChange?.(checked) }} />} /></div>
}

/** 回调地址由站点公开基址和固定路由生成，只允许复制，不参与配置提交。 */
function CallbackPanel({ publicBaseUrl }: { publicBaseUrl?: string | null }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState<string>()
  const resolved = paymentCallbackUrls(publicBaseUrl)
  const callbacks = resolved ? [
    ['stripe', resolved.stripe],
    ['epay', resolved.epay],
    ['return', resolved.return],
  ] as const : []

  const copy = async (key: string, value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(key)
      window.setTimeout(() => setCopied((current) => current === key ? undefined : current), 1_500)
    } catch {
      setCopied(undefined)
    }
  }

  return (
    <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4">
      <h3 className="text-sm font-semibold">{t('paymentSettings.callbacks.title')}</h3>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('paymentSettings.callbacks.description')}</p>
      {callbacks.length ? <div className="mt-3 grid gap-3">
        {callbacks.map(([key, value]) => <div key={key} className="grid gap-1"><label className="text-xs" htmlFor={`payment-callback-${key}`}>{t(`paymentSettings.callbacks.${key}`)}</label><div className="flex gap-1.5"><Input classNames={{ input: 'min-w-0 text-xs' }} id={`payment-callback-${key}`} isReadOnly size="sm" value={value} /><Button isIconOnly aria-label={t('paymentSettings.actions.copy')} size="md" type="button" variant="bordered" onClick={() => void copy(key, value)}>{copied === key ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}</Button></div></div>)}
      </div> : <div className="mt-3 rounded-lg border border-dashed border-[var(--hairline)] bg-surface-2/30 p-3 text-xs leading-5 text-muted-foreground">{t('paymentSettings.callbacks.missingPublicBaseUrl')}</div>}
    </section>
  )
}
