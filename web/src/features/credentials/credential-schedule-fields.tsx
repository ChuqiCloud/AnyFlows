import { Controller, type UseFormReturn } from 'react-hook-form'
import { LoaderCircle, Network } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { useAdminCredentialProxies } from '@/features/credential-proxies/credential-proxy-api'
import { CredentialField } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'

/** 用结构化控件维护账号调度字段，倍率始终以十进制字符串输入。 */
export function CredentialScheduleFields({ form, credential, mode, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  credential?: AdminCredential
  mode: CredentialFormValues['mode']
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const errors = form.formState.errors

  return (
    <div className="grid gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-status" label={t('credentials.schedule.status')} error={errors.status?.message}>
          <Select id="credential-status" disabled={disabled} aria-invalid={Boolean(errors.status)} {...form.register('status')}>
            {credential?.status === 'auto_disabled' ? <option value="auto_disabled" disabled>{t('credentials.status.autoDisabled')}</option> : null}
            <option value="enabled">{t('credentials.status.enabled')}</option>
            <option value="disabled">{t('credentials.status.disabled')}</option>
          </Select>
        </CredentialField>
        <CredentialField id="credential-multi-key-mode" label={t('credentials.schedule.multiKeyMode')}>
          <Select id="credential-multi-key-mode" disabled={disabled} {...form.register('multiKeyMode')}>
            <option value="none">{t('credentials.schedule.modeNone')}</option>
            <option value="random">{t('credentials.schedule.modeRandom')}</option>
            <option value="round_robin">{t('credentials.schedule.modeRoundRobin')}</option>
          </Select>
        </CredentialField>
      </div>

      <div className="flex items-start justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
        <div>
          <Label htmlFor="credential-schedulable">{t('credentials.schedule.schedulable')}</Label>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.schedule.schedulableHint')}</p>
        </div>
        <Controller control={form.control} name="schedulable" render={({ field }) => (
          <Switch id="credential-schedulable" checked={field.value} disabled={disabled} onCheckedChange={field.onChange} />
        )} />
      </div>

      <div className={`grid gap-3 ${mode === 'spark_shadow' ? 'sm:grid-cols-2' : 'sm:grid-cols-3'}`}>
        <NumberField form={form} name="priority" id="credential-priority" label={t('credentials.schedule.priority')} error={errors.priority?.message} disabled={disabled} />
        <NumberField form={form} name="weight" id="credential-weight" label={t('credentials.schedule.weight')} error={errors.weight?.message} min={0} disabled={disabled} />
        {mode === 'standard' ? <NumberField form={form} name="concurrency" id="credential-concurrency" label={t('credentials.schedule.concurrency')} hint={t('credentials.schedule.optional')} error={errors.concurrency?.message} min={0} disabled={disabled} /> : null}
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-load-factor" label={t('credentials.schedule.loadFactor')} hint={t('credentials.schedule.multiplierHint')} error={errors.loadFactor?.message}>
          <Input id="credential-load-factor" inputMode="decimal" placeholder="1" disabled={disabled} aria-invalid={Boolean(errors.loadFactor)} {...form.register('loadFactor')} />
        </CredentialField>
        <CredentialField id="credential-rate-multiplier" label={t('credentials.schedule.rateMultiplier')} hint={t('credentials.schedule.multiplierHint')} error={errors.rateMultiplier?.message}>
          <Input id="credential-rate-multiplier" inputMode="decimal" placeholder="1" disabled={disabled} aria-invalid={Boolean(errors.rateMultiplier)} {...form.register('rateMultiplier')} />
        </CredentialField>
      </div>

      {mode === 'standard' ? <CredentialProxyField form={form} credential={credential} disabled={disabled} /> : null}
    </div>
  )
}

function CredentialProxyField({ form, credential, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  credential?: AdminCredential
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const proxiesQuery = useAdminCredentialProxies()
  const proxies = (proxiesQuery.data ?? []).filter((proxy) => proxy.enabled || proxy.id === credential?.proxy_id)
  const error = form.formState.errors.proxyId?.message
  return (
    <CredentialField id="credential-proxy" label={t('credentials.schedule.proxy')} hint={t('credentials.schedule.proxyHint')} error={error}>
      <div className="relative">
        <Network className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
        <Select id="credential-proxy" className="pl-8" disabled={disabled || proxiesQuery.isPending || proxiesQuery.isError} aria-invalid={Boolean(error)} {...form.register('proxyId')}>
          <option value="">{t('credentials.schedule.noProxy')}</option>
          {credential?.proxy_id && !proxies.some((proxy) => proxy.id === credential.proxy_id) ? <option value={credential.proxy_id}>#{credential.proxy_id}</option> : null}
          {proxies.map((proxy) => <option key={proxy.id} value={proxy.id}>{proxy.name} · {proxy.scheme.toUpperCase()} · {proxy.host}:{proxy.port}</option>)}
        </Select>
        {proxiesQuery.isPending ? <LoaderCircle className="pointer-events-none absolute right-8 top-1/2 size-3.5 -translate-y-1/2 animate-spin text-muted-foreground" aria-hidden="true" /> : null}
      </div>
      {proxiesQuery.isError ? <p className="mt-1 text-[0.6875rem] text-destructive">{t('credentials.schedule.proxyLoadFailed')}</p> : null}
    </CredentialField>
  )
}

function NumberField({ form, name, id, label, hint, error, min, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  name: 'priority' | 'weight' | 'concurrency'
  id: string
  label: string
  hint?: string
  error?: string
  min?: number
  disabled?: boolean
}) {
  return <CredentialField id={id} label={label} hint={hint} error={error}><Input id={id} type="number" step={1} min={min} disabled={disabled} aria-invalid={Boolean(error)} {...form.register(name)} /></CredentialField>
}
