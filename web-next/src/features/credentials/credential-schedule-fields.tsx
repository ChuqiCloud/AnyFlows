import { Controller, type UseFormReturn } from 'react-hook-form'
import { Input, Select, SelectItem, Switch } from '@heroui/react'
import { LoaderCircle, Network } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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
  // HeroUI SelectItem 的 isDisabled 才能表达"不可重新选择"的自动禁用状态。
  const statusItems = [
    ...(credential?.status === 'auto_disabled'
      ? [{ key: 'auto_disabled', label: t('credentials.status.autoDisabled'), isDisabled: true }]
      : []),
    { key: 'enabled', label: t('credentials.status.enabled'), isDisabled: false },
    { key: 'disabled', label: t('credentials.status.disabled'), isDisabled: false },
  ]
  const multiKeyModeItems = [
    { key: 'none', label: t('credentials.schedule.modeNone') },
    { key: 'random', label: t('credentials.schedule.modeRandom') },
    { key: 'round_robin', label: t('credentials.schedule.modeRoundRobin') },
  ]

  return (
    <div className="grid gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-status" label={t('credentials.schedule.status')} error={errors.status?.message}>
          <Controller
            control={form.control}
            name="status"
            render={({ field }) => (
              <Select
                aria-label={t('credentials.schedule.status')}
                id="credential-status"
                isDisabled={disabled}
                isInvalid={Boolean(errors.status)}
                items={statusItems}
                selectedKeys={field.value ? [field.value] : []}
                size="sm"
                onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
              >
                {(item) => <SelectItem key={item.key} isDisabled={item.isDisabled}>{item.label}</SelectItem>}
              </Select>
            )}
          />
        </CredentialField>
        <CredentialField id="credential-multi-key-mode" label={t('credentials.schedule.multiKeyMode')}>
          <Controller
            control={form.control}
            name="multiKeyMode"
            render={({ field }) => (
              <Select
                aria-label={t('credentials.schedule.multiKeyMode')}
                id="credential-multi-key-mode"
                isDisabled={disabled}
                items={multiKeyModeItems}
                selectedKeys={field.value ? [field.value] : []}
                size="sm"
                onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
              >
                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
              </Select>
            )}
          />
        </CredentialField>
      </div>

      <div className="flex items-start justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
        <div>
          <label className="text-xs font-medium leading-none text-foreground" htmlFor="credential-schedulable">{t('credentials.schedule.schedulable')}</label>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.schedule.schedulableHint')}</p>
        </div>
        <Controller control={form.control} name="schedulable" render={({ field }) => (
          <Switch id="credential-schedulable" isDisabled={disabled} isSelected={field.value} size="sm" onValueChange={field.onChange} />
        )} />
      </div>

      <div className={`grid gap-3 ${mode === 'spark_shadow' ? 'sm:grid-cols-2' : 'sm:grid-cols-3'}`}>
        <NumberField form={form} name="priority" id="credential-priority" label={t('credentials.schedule.priority')} error={errors.priority?.message} disabled={disabled} />
        <NumberField form={form} name="weight" id="credential-weight" label={t('credentials.schedule.weight')} error={errors.weight?.message} min={0} disabled={disabled} />
        {mode === 'standard' ? <NumberField form={form} name="concurrency" id="credential-concurrency" label={t('credentials.schedule.concurrency')} hint={t('credentials.schedule.optional')} error={errors.concurrency?.message} min={0} disabled={disabled} /> : null}
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-load-factor" label={t('credentials.schedule.loadFactor')} hint={t('credentials.schedule.multiplierHint')} error={errors.loadFactor?.message}>
          <Input id="credential-load-factor" inputMode="decimal" isDisabled={disabled} isInvalid={Boolean(errors.loadFactor)} placeholder="1" size="sm" {...form.register('loadFactor')} />
        </CredentialField>
        <CredentialField id="credential-rate-multiplier" label={t('credentials.schedule.rateMultiplier')} hint={t('credentials.schedule.multiplierHint')} error={errors.rateMultiplier?.message}>
          <Input id="credential-rate-multiplier" inputMode="decimal" isDisabled={disabled} isInvalid={Boolean(errors.rateMultiplier)} placeholder="1" size="sm" {...form.register('rateMultiplier')} />
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
  const proxiesUnavailable = disabled || proxiesQuery.isPending || proxiesQuery.isError
  // 保留已停用但仍被引用的代理，避免编辑时静默改绑。
  const retainedProxyId = credential?.proxy_id && !proxies.some((proxy) => proxy.id === credential.proxy_id)
    ? credential.proxy_id
    : undefined
  const proxyItems = [
    { key: NO_PROXY_KEY, label: t('credentials.schedule.noProxy') },
    ...(retainedProxyId !== undefined
      ? [{ key: String(retainedProxyId), label: `#${retainedProxyId}` }]
      : []),
    ...proxies.map((proxy) => ({ key: String(proxy.id), label: `${proxy.name} · ${proxy.scheme.toUpperCase()} · ${proxy.host}:${proxy.port}` })),
  ]
  return (
    <CredentialField id="credential-proxy" label={t('credentials.schedule.proxy')} hint={t('credentials.schedule.proxyHint')} error={error}>
      <div className="relative">
        <Network className="pointer-events-none absolute left-2.5 top-1/2 z-10 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
        <Controller
          control={form.control}
          name="proxyId"
          render={({ field }) => (
            <Select
              aria-label={t('credentials.schedule.proxy')}
              classNames={{ trigger: 'pl-8' }}
              id="credential-proxy"
              isDisabled={proxiesUnavailable}
              isInvalid={Boolean(error)}
              items={proxyItems}
              selectedKeys={[field.value === '' ? NO_PROXY_KEY : field.value]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = String(Array.from(keys)[0] ?? NO_PROXY_KEY)
                field.onChange(next === NO_PROXY_KEY ? '' : next)
              }}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          )}
        />
        {proxiesQuery.isPending ? <LoaderCircle className="pointer-events-none absolute right-8 top-1/2 z-10 size-3.5 -translate-y-1/2 animate-spin text-muted-foreground" aria-hidden="true" /> : null}
      </div>
      {proxiesQuery.isError ? <p className="mt-1 text-[0.6875rem] text-destructive">{t('credentials.schedule.proxyLoadFailed')}</p> : null}
    </CredentialField>
  )
}

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不设置代理"。 */
const NO_PROXY_KEY = '__none__'

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
  return <CredentialField id={id} label={label} hint={hint} error={error}><Input id={id} inputMode="numeric" isDisabled={disabled} isInvalid={Boolean(error)} min={min} size="sm" step={1} type="number" {...form.register(name)} /></CredentialField>
}
