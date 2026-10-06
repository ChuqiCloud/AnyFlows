import { GitFork, KeyRound, Network, TriangleAlert, Users } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { UseFormReturn } from 'react-hook-form'

import { Badge } from '@/components/ui/badge'
import { Select } from '@/components/ui/select'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { CredentialField } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'
import {
  sparkShadowParentBlocked,
  sparkShadowParentCandidates,
} from './credential-model'

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

  return (
    <div className="grid gap-3">
      {credential ? (
        <div className="flex items-center justify-between gap-3 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
          <div className="min-w-0">
            <p className="text-xs font-medium">{t('credentials.spark.parent')}</p>
            <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.spark.parentLocked')}</p>
          </div>
          <Badge className="shrink-0">#{credential.parent_id ?? '?'}</Badge>
        </div>
      ) : (
        <CredentialField id="credential-spark-parent" label={t('credentials.spark.parent')} hint={t('credentials.spark.parentHint')} error={errors.parentId?.message}>
          <Select id="credential-spark-parent" disabled={disabled || candidates.length === 0} aria-invalid={Boolean(errors.parentId)} {...form.register('parentId')}>
            <option value="">{t('credentials.spark.selectParent')}</option>
            {candidates.map((item) => (
              <option key={item.id} value={item.id}>
                #{item.id} · {item.oauth_provider ?? t('credentials.oauth.unbound')}
              </option>
            ))}
          </Select>
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
          {parentBlocked ? <Badge className="shrink-0 border-transparent bg-destructive/10 text-destructive">{t('credentials.runtime.parentBlocked')}</Badge> : null}
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
