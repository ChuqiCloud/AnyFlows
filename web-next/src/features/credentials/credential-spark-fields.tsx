import { Chip, Select, SelectItem } from '@heroui/react'
import { GitFork, KeyRound, Network, TriangleAlert, Users } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Controller, type UseFormReturn } from 'react-hook-form'

import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { CredentialField } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'
import {
  sparkShadowParentBlocked,
  sparkShadowParentCandidates,
} from './credential-model'

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不指定母凭据"。 */
const NO_PARENT_KEY = '__none__'

/** 选择唯一母凭据并预览影子实际继承的敏感身份、代理和并发。 */
export function CredentialSparkFields({ form, credentials, credential, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  credentials: readonly AdminCredential[]
  credential?: AdminCredential
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const candidates = sparkShadowParentCandidates(credentials, credential?.id)
  const parentId = Number(form.watch('parentId'))
  const parent = credentials.find((item) => item.id === parentId)
  const existingParent = credential?.parent_id
    ? credentials.find((item) => item.id === credential.parent_id)
    : undefined
  const selectedParent = parent ?? existingParent
  const parentBlocked = credential
    ? sparkShadowParentBlocked(selectedParent)
    : selectedParent ? sparkShadowParentBlocked(selectedParent) : false
  const errors = form.formState.errors
  // HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不指定母凭据"。
  const parentItems = [
    { key: NO_PARENT_KEY, label: t('credentials.spark.selectParent') },
    ...candidates.map((item) => ({
      key: String(item.id),
      label: `#${item.id} · ${item.oauth_provider ?? t('credentials.oauth.unbound')}`,
    })),
  ]

  return (
    <div className="grid gap-3">
      {credential ? (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
          <div className="min-w-0">
            <p className="text-xs font-medium">{t('credentials.spark.parent')}</p>
            <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.spark.parentLocked')}</p>
          </div>
          <Chip className="shrink-0" size="sm" variant="flat">#{credential.parent_id ?? '?'}</Chip>
        </div>
      ) : (
        <CredentialField id="credential-spark-parent" label={t('credentials.spark.parent')} hint={t('credentials.spark.parentHint')} error={errors.parentId?.message}>
          <Controller
            control={form.control}
            name="parentId"
            render={({ field }) => (
              <Select
                aria-label={t('credentials.spark.parent')}
                id="credential-spark-parent"
                isDisabled={disabled || candidates.length === 0}
                isInvalid={Boolean(errors.parentId)}
                items={parentItems}
                selectedKeys={[field.value === '' ? NO_PARENT_KEY : field.value]}
                size="sm"
                onSelectionChange={(keys) => {
                  const next = String(Array.from(keys)[0] ?? NO_PARENT_KEY)
                  field.onChange(next === NO_PARENT_KEY ? '' : next)
                }}
              >
                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
              </Select>
            )}
          />
        </CredentialField>
      )}

      {!credential && candidates.length === 0 ? (
        <div role="status" className="flex items-start gap-2.5 rounded-lg bg-warning/8 px-3 py-2.5 text-warning">
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p className="text-xs leading-5">{t('credentials.spark.noParent')}</p>
        </div>
      ) : null}

      <div className="grid gap-2 rounded-lg border border-[var(--hairline)] px-3 py-3">
        <div className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2 text-xs font-medium"><GitFork className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />{t('credentials.spark.inheritance')}</div>
          {parentBlocked ? <Chip className="shrink-0 bg-destructive/10 text-destructive" size="sm" variant="flat">{t('credentials.runtime.parentBlocked')}</Chip> : null}
        </div>
        <InheritanceRow icon={KeyRound} label={t('credentials.spark.secret')} value={selectedParent ? t('credentials.spark.fromParent', { id: selectedParent.id }) : t('credentials.spark.selectFirst')} />
        <InheritanceRow icon={Network} label={t('credentials.spark.proxy')} value={selectedParent ? selectedParent.proxy_id ? `#${selectedParent.proxy_id}` : t('credentials.spark.globalProxy') : t('credentials.spark.selectFirst')} />
        <InheritanceRow icon={Users} label={t('credentials.spark.concurrency')} value={selectedParent ? String(selectedParent.concurrency ?? t('credentials.values.unlimited')) : t('credentials.spark.selectFirst')} />
      </div>
    </div>
  )
}

function InheritanceRow({ icon: Icon, label, value }: {
  icon: typeof KeyRound
  label: string
  value: string
}) {
  return (
    <div className="grid grid-cols-[1fr_auto] items-center gap-3 text-[0.6875rem]">
      <span className="flex min-w-0 items-center gap-2 text-muted-foreground"><Icon className="size-3.5 shrink-0" aria-hidden="true" />{label}</span>
      <span className="max-w-48 truncate text-right font-medium tabular-nums" title={value}>{value}</span>
    </div>
  )
}
