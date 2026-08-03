import { Controller, type UseFormReturn } from 'react-hook-form'
import { Network } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { CredentialField } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'
import { credentialParentCandidates } from './credential-model'

/** 用结构化控件维护账号调度字段，倍率始终以十进制字符串输入。 */
export function CredentialScheduleFields({ form, credentials, credential, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  credentials: readonly AdminCredential[]
  credential?: AdminCredential
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const kind = form.watch('kind')
  const parents = credentialParentCandidates(credentials, kind, credential?.id)
  const existingParent = credential?.parent_id === null || credential?.parent_id === undefined
    ? undefined
    : credentials.find((item) => item.id === credential.parent_id)
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

      <div className="grid gap-3 sm:grid-cols-3">
        <NumberField form={form} name="priority" id="credential-priority" label={t('credentials.schedule.priority')} error={errors.priority?.message} disabled={disabled} />
        <NumberField form={form} name="weight" id="credential-weight" label={t('credentials.schedule.weight')} error={errors.weight?.message} min={0} disabled={disabled} />
        <NumberField form={form} name="concurrency" id="credential-concurrency" label={t('credentials.schedule.concurrency')} hint={t('credentials.schedule.optional')} error={errors.concurrency?.message} min={0} disabled={disabled} />
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-load-factor" label={t('credentials.schedule.loadFactor')} hint={t('credentials.schedule.multiplierHint')} error={errors.loadFactor?.message}>
          <Input id="credential-load-factor" inputMode="decimal" placeholder="1" disabled={disabled} aria-invalid={Boolean(errors.loadFactor)} {...form.register('loadFactor')} />
        </CredentialField>
        <CredentialField id="credential-rate-multiplier" label={t('credentials.schedule.rateMultiplier')} hint={t('credentials.schedule.multiplierHint')} error={errors.rateMultiplier?.message}>
          <Input id="credential-rate-multiplier" inputMode="decimal" placeholder="1" disabled={disabled} aria-invalid={Boolean(errors.rateMultiplier)} {...form.register('rateMultiplier')} />
        </CredentialField>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-quota-dimension" label={t('credentials.schedule.quotaDimension')}>
          <Select id="credential-quota-dimension" disabled={disabled} {...form.register('quotaDimension')}>
            <option value="global">{t('credentials.schedule.quotaGlobal')}</option>
            <option value="spark">{t('credentials.schedule.quotaSpark')}</option>
          </Select>
        </CredentialField>
        <CredentialField id="credential-parent" label={t('credentials.schedule.parent')} hint={t('credentials.schedule.parentHint')} error={errors.parentId?.message}>
          <Select id="credential-parent" disabled={disabled} aria-invalid={Boolean(errors.parentId)} {...form.register('parentId')}>
            <option value="">{t('credentials.schedule.noParent')}</option>
            {existingParent && !parents.some((item) => item.id === existingParent.id) ? <option value={existingParent.id}>#{existingParent.id}</option> : null}
            {credential?.parent_id && !existingParent ? <option value={credential.parent_id}>#{credential.parent_id}</option> : null}
            {parents.map((item) => <option key={item.id} value={item.id}>#{item.id} · {t(`credentials.kind.${item.kind}`)}</option>)}
          </Select>
        </CredentialField>
      </div>

      {credential?.proxy_id ? (
        <div className="flex items-center justify-between gap-3 rounded-lg bg-surface-2/55 px-3 py-2.5">
          <div className="flex items-center gap-2 text-xs"><Network className="size-4 text-muted-foreground" aria-hidden="true" />{t('credentials.schedule.proxyBound')}</div>
          <Badge>#{credential.proxy_id}</Badge>
        </div>
      ) : null}
    </div>
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
