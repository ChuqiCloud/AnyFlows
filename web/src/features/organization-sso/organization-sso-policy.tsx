import { CircleAlert, KeyRound, LoaderCircle, RefreshCw, ShieldCheck, ShieldX } from 'lucide-react'
import { useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import type { OrganizationSsoPolicyResponse } from '@/lib/api/generated/types.gen'
import {
  organizationSsoErrorCode,
  useEstablishOrganizationSsoRecovery,
  useOrganizationSsoPolicy,
  useSuspendOrganizationSsoRequired,
  useUpdateOrganizationSsoPolicy,
} from './organization-sso-api'

const SSO_READ = 'organization.sso.read'
const SSO_RECOVERY = 'organization.sso.recovery'

/** 展示并管理企业 SSO 强制策略；所有可执行状态均以服务端事实为准。 */
export function OrganizationSsoPolicyPanel({
  organizationId,
  ssoEnabled,
  permissions,
}: {
  organizationId: number
  ssoEnabled: boolean
  permissions: readonly string[]
}) {
  const { t } = useTranslation()
  const canRead = permissions.includes(SSO_READ)
  const canRecover = permissions.includes(SSO_RECOVERY)
  const query = useOrganizationSsoPolicy(organizationId, ssoEnabled && canRead)
  const policy = query.data

  return (
    <section className="border-t border-[var(--hairline)]">
      <header className="border-b border-[var(--hairline)] bg-surface-1/45 px-4 py-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-warning/10 text-warning"><KeyRound className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h2 className="text-base font-semibold">{t('organizationConsole.settings.sso.policy.title')}</h2>
                {policy ? <Badge className={policy.policy === 'required' ? 'text-warning' : 'text-success'}>{t(`organizationConsole.settings.sso.policy.mode.${policy.policy}`)}</Badge> : null}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.policy.subtitle')}</p>
            </div>
          </div>
          {ssoEnabled && canRead ? (
            <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={query.isFetching} onClick={() => void query.refetch()}>
              <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
              <span className="sr-only">{t('organizationConsole.actions.refresh')}</span>
            </Button>
          ) : null}
        </div>
      </header>
      <div className="p-0">
        {!ssoEnabled ? <PolicyState icon={ShieldX} title={t('organizationConsole.settings.sso.restricted.entitlementTitle')} body={t('organizationConsole.settings.sso.restricted.entitlementBody')} /> : null}
        {ssoEnabled && !canRead ? <PolicyState icon={ShieldX} title={t('organizationConsole.settings.sso.policy.restrictedTitle')} body={t('organizationConsole.settings.sso.policy.restrictedBody')} /> : null}
        {ssoEnabled && canRead && query.isPending ? <PolicyLoading /> : null}
        {ssoEnabled && canRead && query.isError ? <PolicyState icon={CircleAlert} title={t('organizationConsole.settings.sso.policy.errors.title')} body={t('organizationConsole.settings.sso.policy.errors.body')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} /> : null}
        {ssoEnabled && canRead && query.isSuccess && policy ? (
          <PolicyContent organizationId={organizationId} policy={policy} canManage={canRecover} onRefresh={() => void query.refetch()} />
        ) : null}
      </div>
    </section>
  )
}

function PolicyContent({
  organizationId,
  policy,
  canManage,
  onRefresh,
}: {
  organizationId: number
  policy: OrganizationSsoPolicyResponse
  canManage: boolean
  onRefresh: () => void
}) {
  const { t } = useTranslation()
  const updateMutation = useUpdateOrganizationSsoPolicy(organizationId)
  const establishMutation = useEstablishOrganizationSsoRecovery(organizationId)
  const suspendMutation = useSuspendOrganizationSsoRequired(organizationId)
  const [mode, setMode] = useState(policy.policy)
  const [password, setPassword] = useState('')
  const [totp, setTotp] = useState('')
  const [feedback, setFeedback] = useState<'success' | undefined>()
  const busy = updateMutation.isPending || establishMutation.isPending || suspendMutation.isPending
  const mutationError = updateMutation.error ?? establishMutation.error ?? suspendMutation.error
  const errorCode = organizationSsoErrorCode(mutationError)
  const needsRecovery = mode === 'required' && !policy.recovery_ready

  const updatePolicy = async () => {
    if (!canManage || mode === policy.policy || (mode === 'required' && !policy.recovery_ready)) return
    setFeedback(undefined)
    try {
      await updateMutation.mutateAsync({ expected_version: policy.version, policy: mode })
      setFeedback('success')
    } catch {
      // 保留当前选择，便于 CAS 冲突后刷新事实并重试。
    }
  }

  const submitRecovery = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canManage || !password.trim()) return
    setFeedback(undefined)
    try {
      const body = { current_password: password, expected_version: policy.version, ...(totp.trim() ? { totp_code: totp } : {}) }
      if (policy.policy === 'required') await suspendMutation.mutateAsync(body)
      else await establishMutation.mutateAsync(body)
      setPassword('')
      setTotp('')
      setFeedback('success')
    } catch {
      // 敏感输入保留在表单中，错误详情由稳定错误码决定。
    }
  }

  return (
    <div className="divide-y divide-[var(--hairline)]">
      <div className="grid gap-3 p-4 sm:grid-cols-3">
        <Fact label={t('organizationConsole.settings.sso.policy.fields.mode')} value={t(`organizationConsole.settings.sso.policy.mode.${policy.policy}`)} />
        <Fact label={t('organizationConsole.settings.sso.policy.fields.version')} value={`v${policy.version}`} mono />
        <Fact label={t('organizationConsole.settings.sso.policy.fields.recovery')} value={t(policy.recovery_ready ? 'organizationConsole.settings.sso.policy.ready' : 'organizationConsole.settings.sso.policy.notReady')} />
      </div>
      {canManage ? (
        <div className="grid gap-4 bg-surface-1/25 p-4">
          <div className="grid gap-1.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
            <div className="grid gap-1.5">
              <Label htmlFor="organization-sso-policy-mode">{t('organizationConsole.settings.sso.policy.modeLabel')}</Label>
              <Select id="organization-sso-policy-mode" value={mode} disabled={busy} onChange={(event) => setMode(event.target.value as typeof mode)}>
                <option value="optional">{t('organizationConsole.settings.sso.policy.mode.optional')}</option>
                <option value="required" disabled={!policy.recovery_ready}>{t('organizationConsole.settings.sso.policy.mode.required')}</option>
              </Select>
            </div>
            <Button type="button" disabled={busy || mode === policy.policy || needsRecovery} onClick={() => void updatePolicy()}><ShieldCheck aria-hidden="true" />{t('organizationConsole.settings.sso.policy.save')}</Button>
          </div>
          {needsRecovery ? <p className="text-xs leading-5 text-warning">{t('organizationConsole.settings.sso.policy.recoveryRequired')}</p> : null}
          <form className="grid gap-3 border-t border-[var(--hairline)] pt-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,0.7fr)_auto] sm:items-end" onSubmit={(event) => void submitRecovery(event)}>
            <div className="grid gap-1.5">
              <Label htmlFor="organization-sso-recovery-password">{t('organizationConsole.settings.sso.policy.password')}</Label>
              <Input id="organization-sso-recovery-password" type="password" autoComplete="current-password" value={password} disabled={busy} onChange={(event) => setPassword(event.target.value)} />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="organization-sso-recovery-totp">{t('organizationConsole.settings.sso.policy.totp')}</Label>
              <Input id="organization-sso-recovery-totp" inputMode="numeric" autoComplete="one-time-code" value={totp} disabled={busy} onChange={(event) => setTotp(event.target.value)} />
            </div>
            <Button type="submit" variant={policy.policy === 'required' ? 'outline' : 'secondary'} disabled={busy || !password.trim()}>
              {busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <KeyRound aria-hidden="true" />}
              {t(policy.policy === 'required' ? 'organizationConsole.settings.sso.policy.suspend' : 'organizationConsole.settings.sso.policy.establish')}
            </Button>
          </form>
          <div className="flex flex-wrap items-center gap-3 text-xs">
            {feedback === 'success' ? <span role="status" className="text-success">{t('organizationConsole.settings.sso.policy.saved')}</span> : null}
            {mutationError ? <span role="alert" className="text-destructive">{t(`organizationConsole.settings.sso.policy.errors.${errorCode === 'organization_sso_recovery_conflict' ? 'conflict' : errorCode === 'organization_sso_recovery_rejected' ? 'rejected' : 'mutation'}`)}</span> : null}
            {mutationError ? <Button type="button" size="sm" variant="ghost" onClick={onRefresh}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button> : null}
          </div>
        </div>
      ) : (
        <p className="bg-surface-1/25 px-4 py-3 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.policy.readOnly')}</p>
      )}
    </div>
  )
}

function Fact({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="min-w-0 rounded-lg border border-[var(--hairline)] px-3 py-2"><div className="text-[0.6875rem] text-muted-foreground">{label}</div><div className={`mt-1 truncate text-sm font-medium ${mono ? 'font-mono' : ''}`}>{value}</div></div>
}

function PolicyLoading() {
  return <div className="grid gap-3 p-4" role="status"><div className="h-10 animate-pulse rounded-lg bg-surface-2" /><div className="h-24 animate-pulse rounded-lg bg-surface-2" /></div>
}

function PolicyState({ icon: Icon, title, body, action }: { icon: typeof ShieldX; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-36 place-items-center p-6 text-center"><div className="max-w-md"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
