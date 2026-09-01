import { ArrowUpRight, CircleAlert, Fingerprint, LoaderCircle, LogIn, RefreshCw, ShieldBan, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useState, type ReactNode } from 'react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { useOrganizationAuditLogs } from '@/features/organization-activity/organization-activity-api'
import type { OrganizationSsoProvider } from '@/lib/api/generated/types.gen'
import {
  useOrganizationSsoDomains,
  useOrganizationSsoPolicy,
  useOrganizationSsoProviders,
  useStartOrganizationSsoLogin,
} from './organization-sso-api'
import { createOrganizationSsoRequestDigest, submitOrganizationSsoSamlAuthnRequest } from './organization-sso-authn-request'
import { OrganizationSsoDomains } from './organization-sso-domains'
import { OrganizationSsoPolicyPanel } from './organization-sso-policy'

const SSO_READ = 'organization.sso.read'
const SSO_MANAGE = 'organization.sso.manage'
const AUDIT_READ = 'organization.audit.read'

type ClosureStatus = 'live' | 'empty' | 'pending' | 'loading' | 'error' | 'restricted' | 'revoked' | 'notReady'

/** 企业设置中的 SSO 脱敏状态面板；所有状态都来自服务端权限和事实投影。 */
export function OrganizationSsoPanel({
  organizationId,
  organizationName,
  ssoEnabled,
  permissions,
}: {
  organizationId: number
  organizationName: string
  ssoEnabled: boolean
  permissions: readonly string[]
}) {
  const { t } = useTranslation()
  const canRead = permissions.includes(SSO_READ)
  const canManage = permissions.includes(SSO_MANAGE)
  const canAudit = permissions.includes(AUDIT_READ)
  const enabled = ssoEnabled && canRead
  const providersQuery = useOrganizationSsoProviders(organizationId, enabled)
  const domainsQuery = useOrganizationSsoDomains(organizationId, enabled)
  const policyQuery = useOrganizationSsoPolicy(organizationId, enabled)
  const startMutation = useStartOrganizationSsoLogin(organizationId)
  const [startError, setStartError] = useState<'invalid' | 'unavailable'>()
  const auditQuery = useOrganizationAuditLogs(organizationId, undefined, 1, {}, ssoEnabled && canAudit)
  const providers = providersQuery.data ?? []
  async function startSamlLogin(providerKey: string) {
    setStartError(undefined)
    try {
      const response = await startMutation.mutateAsync({
        provider_key: providerKey,
        request_digest: createOrganizationSsoRequestDigest(),
      })
      if (response.protocol !== 'saml' || !submitOrganizationSsoSamlAuthnRequest(response)) {
        setStartError('invalid')
      }
    } catch {
      setStartError('unavailable')
    }
  }

  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex min-w-0 items-start gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><Fingerprint className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <CardTitle className="text-base">{t('organizationConsole.settings.sso.title')}</CardTitle>
                <Badge className={ssoEnabled ? 'text-success' : 'text-muted-foreground'}>{t(ssoEnabled ? 'organizationConsole.settings.sso.enabled' : 'organizationConsole.settings.sso.disabled')}</Badge>
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.subtitle', { organization: organizationName })}</p>
            </div>
          </div>
          {enabled ? (
            <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={providersQuery.isFetching || domainsQuery.isFetching || policyQuery.isFetching || auditQuery.isFetching} onClick={() => void Promise.all([providersQuery.refetch(), domainsQuery.refetch(), policyQuery.refetch(), auditQuery.refetch()])}>
              <RefreshCw className={providersQuery.isFetching || domainsQuery.isFetching || policyQuery.isFetching || auditQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
              <span className="sr-only">{t('organizationConsole.actions.refresh')}</span>
            </Button>
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {!ssoEnabled ? <SsoPanelState icon={ShieldBan} title={t('organizationConsole.settings.sso.restricted.entitlementTitle')} body={t('organizationConsole.settings.sso.restricted.entitlementBody')} /> : null}
        {ssoEnabled && !canRead ? <SsoPanelState icon={ShieldBan} title={t('organizationConsole.settings.sso.restricted.permissionTitle')} body={t('organizationConsole.settings.sso.restricted.permissionBody')} /> : null}
        {enabled && providersQuery.isPending ? <SsoPanelLoading /> : null}
        {enabled && providersQuery.isError ? <SsoPanelState icon={CircleAlert} title={t('organizationConsole.settings.sso.errors.title')} body={t(canManage ? 'organizationConsole.settings.sso.errors.body' : 'organizationConsole.settings.sso.errors.readOnlyBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void providersQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} /> : null}
        {enabled && providersQuery.isSuccess && providers.length === 0 ? <SsoPanelState icon={ShieldCheck} title={t('organizationConsole.settings.sso.empty.title')} body={t(canManage ? 'organizationConsole.settings.sso.empty.manageBody' : 'organizationConsole.settings.sso.empty.readBody')} /> : null}
        {enabled && providers.length > 0 ? <div className="divide-y divide-[var(--hairline)]">{providers.map((provider) => <SsoProviderRow key={provider.provider_key} provider={provider} canManage={canManage} startDisabled={startMutation.isPending} starting={startMutation.isPending && startMutation.variables?.provider_key === provider.provider_key} onStart={() => void startSamlLogin(provider.provider_key)} />)}</div> : null}
        {startError ? <p role="alert" className="border-t border-[var(--hairline)] px-4 py-3 text-xs leading-5 text-destructive">{t(`organizationConsole.settings.sso.login.${startError}`)}</p> : null}
        {enabled ? <OrganizationSsoDomains organizationId={organizationId} providers={providers} canManage={canManage} providersLoaded={providersQuery.isSuccess} query={domainsQuery} /> : null}
        {enabled ? <OrganizationSsoPolicyPanel organizationId={organizationId} ssoEnabled={ssoEnabled} permissions={permissions} query={policyQuery} /> : null}
        <SsoClosureStatus
          canAudit={canAudit}
          domains={domainsQuery.data ?? []}
          domainsQuery={domainsQuery}
          enabled={enabled}
          onAudit={() => { window.location.hash = '#/console/organization-activity?tab=audit' }}
          policy={policyQuery.data}
          policyQuery={policyQuery}
          providers={providers}
          providersQuery={providersQuery}
          auditQuery={auditQuery}
          ssoEnabled={ssoEnabled}
        />
      </CardContent>
    </Card>
  )
}

function SsoClosureStatus({
  canAudit,
  domains,
  domainsQuery,
  enabled,
  onAudit,
  policy,
  policyQuery,
  providers,
  providersQuery,
  auditQuery,
  ssoEnabled,
}: {
  canAudit: boolean
  domains: readonly { status: string }[]
  domainsQuery: ReturnType<typeof useOrganizationSsoDomains>
  enabled: boolean
  onAudit: () => void
  policy: { recovery_ready: boolean } | undefined
  policyQuery: ReturnType<typeof useOrganizationSsoPolicy>
  providers: readonly OrganizationSsoProvider[]
  providersQuery: ReturnType<typeof useOrganizationSsoProviders>
  auditQuery: ReturnType<typeof useOrganizationAuditLogs>
  ssoEnabled: boolean
}) {
  const { t } = useTranslation()
  const items: Array<{ id: 'provider' | 'domains' | 'policy' | 'recovery' | 'audit'; status: ClosureStatus; action?: ReactNode }> = [
    { id: 'provider', status: queryClosureStatus(enabled, providersQuery.isPending, providersQuery.isError, providers.length > 0) },
    { id: 'domains', status: domainClosureStatus(enabled, domainsQuery, domains) },
    { id: 'policy', status: queryClosureStatus(enabled, policyQuery.isPending, policyQuery.isError, policy !== undefined) },
    { id: 'recovery', status: queryClosureStatus(enabled, policyQuery.isPending, policyQuery.isError, policy?.recovery_ready === true, 'notReady') },
    { id: 'audit', status: queryClosureStatus(ssoEnabled && canAudit, auditQuery.isPending, auditQuery.isError, auditQuery.isSuccess), action: canAudit && ssoEnabled ? <Button asChild type="button" size="sm" variant="link" className="h-6 px-1.5 text-xs"><a href="#/console/organization-activity?tab=audit" onClick={onAudit}>{t('organizationConsole.settings.sso.closure.viewAudit')}<ArrowUpRight aria-hidden="true" /></a></Button> : undefined },
  ]

  return (
    <div className="border-t border-[var(--hairline)] bg-surface-1/25 px-4 py-3">
      <div className="mb-2 text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-muted-foreground">{t('organizationConsole.settings.sso.closure.title')}</div>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
        {items.map((item) => (
          <div key={item.id} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-dashed border-[var(--hairline)] px-3 py-2">
            <span className="truncate text-xs">{t(`organizationConsole.settings.sso.closure.${item.id}`)}</span>
            <div className="flex shrink-0 items-center gap-1">
              <Badge className={closureStatusClass(item.status)}>{t(`organizationConsole.settings.sso.closure.status.${item.status}`)}</Badge>
              {item.action}
            </div>
          </div>
        ))}
      </div>
      <p className="mt-2 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.closure.hint')}</p>
    </div>
  )
}

function queryClosureStatus(enabled: boolean, pending: boolean, error: boolean, ready: boolean, empty: ClosureStatus = 'empty'): ClosureStatus {
  if (!enabled) return 'restricted'
  if (pending) return 'loading'
  if (error) return 'error'
  return ready ? 'live' : empty
}

function domainClosureStatus(enabled: boolean, query: ReturnType<typeof useOrganizationSsoDomains>, domains: readonly { status: string }[]): ClosureStatus {
  if (!enabled) return 'restricted'
  if (query.isPending) return 'loading'
  if (query.isError) return 'error'
  if (domains.some((domain) => domain.status === 'verified')) return 'live'
  if (domains.some((domain) => domain.status === 'pending')) return 'pending'
  if (domains.some((domain) => domain.status === 'revoked')) return 'revoked'
  return 'empty'
}

function closureStatusClass(status: ClosureStatus) {
  if (status === 'live') return 'shrink-0 border-transparent bg-success/10 text-success'
  if (status === 'error') return 'shrink-0 border-transparent bg-destructive/10 text-destructive'
  if (status === 'pending' || status === 'notReady') return 'shrink-0 border-transparent bg-warning/10 text-warning'
  return 'shrink-0 text-muted-foreground'
}

function SsoProviderRow({ provider, canManage, startDisabled, starting, onStart }: { provider: OrganizationSsoProvider; canManage: boolean; startDisabled: boolean; starting: boolean; onStart: () => void }) {
  const { t } = useTranslation()
  const active = provider.status === 'active'
  return (
    <article className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="min-w-0 truncate text-sm font-medium" title={provider.display_name}>{provider.display_name}</h3>
          <Badge className={active ? 'text-success' : 'text-muted-foreground'}>{t(`organizationConsole.settings.sso.status.${provider.status}`, { defaultValue: provider.status })}</Badge>
        </div>
        <p className="mt-1 truncate font-mono text-xs text-muted-foreground" title={provider.provider_key}>{provider.provider_key}</p>
      </div>
      <div className="flex flex-wrap items-start justify-between gap-3 sm:min-w-52 sm:justify-end">
        <dl className="grid grid-cols-2 gap-x-5 gap-y-1 text-xs">
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.protocol')}</dt><dd className="font-medium">{t(`organizationConsole.settings.sso.protocol.${provider.protocol}`, { defaultValue: provider.protocol })}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.version')}</dt><dd className="font-mono font-medium">v{provider.version}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.secret')}</dt><dd className="font-medium">{t(provider.secret_configured ? 'organizationConsole.settings.sso.configured' : 'organizationConsole.settings.sso.notConfigured')}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.metadata')}</dt><dd className="font-medium">{t(provider.metadata_projection ? 'organizationConsole.settings.sso.configured' : 'organizationConsole.settings.sso.notConfigured')}</dd></div>
        </dl>
        {canManage && active && provider.protocol === 'saml' ? <Button type="button" size="sm" variant="secondary" disabled={startDisabled} onClick={onStart}>{starting ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <LogIn className="size-3.5" aria-hidden="true" />}{starting ? t('organizationConsole.settings.sso.login.starting') : t('organizationConsole.settings.sso.login.start')}</Button> : null}
      </div>
    </article>
  )
}

function SsoPanelLoading() {
  return <div className="grid gap-3 p-4" role="status"><Skeleton className="h-16" /><Skeleton className="h-16" /></div>
}

function SsoPanelState({ icon: Icon, title, body, action }: { icon: typeof ShieldBan; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-36 place-items-center p-6 text-center"><div className="max-w-md"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
