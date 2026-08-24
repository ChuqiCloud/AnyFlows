import { CircleAlert, Fingerprint, RefreshCw, ShieldBan, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { ReactNode } from 'react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import type { OrganizationSsoProvider } from '@/lib/api/generated/types.gen'
import { useOrganizationSsoProviders } from './organization-sso-api'

const SSO_READ = 'organization.sso.read'
const SSO_MANAGE = 'organization.sso.manage'

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
  const query = useOrganizationSsoProviders(organizationId, ssoEnabled && canRead)
  const providers = query.data ?? []

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
          {ssoEnabled && canRead ? (
            <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={query.isFetching} onClick={() => void query.refetch()}>
              <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
              <span className="sr-only">{t('organizationConsole.actions.refresh')}</span>
            </Button>
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {!ssoEnabled ? <SsoPanelState icon={ShieldBan} title={t('organizationConsole.settings.sso.restricted.entitlementTitle')} body={t('organizationConsole.settings.sso.restricted.entitlementBody')} /> : null}
        {ssoEnabled && !canRead ? <SsoPanelState icon={ShieldBan} title={t('organizationConsole.settings.sso.restricted.permissionTitle')} body={t('organizationConsole.settings.sso.restricted.permissionBody')} /> : null}
        {ssoEnabled && canRead && query.isPending ? <SsoPanelLoading /> : null}
        {ssoEnabled && canRead && query.isError ? <SsoPanelState icon={CircleAlert} title={t('organizationConsole.settings.sso.errors.title')} body={t(canManage ? 'organizationConsole.settings.sso.errors.body' : 'organizationConsole.settings.sso.errors.readOnlyBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} /> : null}
        {ssoEnabled && canRead && query.isSuccess && providers.length === 0 ? <SsoPanelState icon={ShieldCheck} title={t('organizationConsole.settings.sso.empty.title')} body={t(canManage ? 'organizationConsole.settings.sso.empty.manageBody' : 'organizationConsole.settings.sso.empty.readBody')} /> : null}
        {ssoEnabled && canRead && providers.length > 0 ? <div className="divide-y divide-[var(--hairline)]">{providers.map((provider) => <SsoProviderRow key={provider.provider_key} provider={provider} />)}</div> : null}
        <div className="border-t border-[var(--hairline)] bg-surface-1/25 px-4 py-3">
          <div className="mb-2 text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-muted-foreground">{t('organizationConsole.settings.sso.closure.title')}</div>
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
            {(['domains', 'policy', 'recovery', 'audit'] as const).map((item) => (
              <div key={item} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-dashed border-[var(--hairline)] px-3 py-2">
                <span className="truncate text-xs">{t(`organizationConsole.settings.sso.closure.${item}`)}</span>
                <Badge className="shrink-0 text-muted-foreground">{t('organizationConsole.settings.sso.closure.pending')}</Badge>
              </div>
            ))}
          </div>
          <p className="mt-2 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.closure.hint')}</p>
        </div>
      </CardContent>
    </Card>
  )
}

function SsoProviderRow({ provider }: { provider: OrganizationSsoProvider }) {
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
      <dl className="grid grid-cols-2 gap-x-5 gap-y-1 text-xs sm:min-w-52">
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.protocol')}</dt><dd className="font-medium">{t(`organizationConsole.settings.sso.protocol.${provider.protocol}`, { defaultValue: provider.protocol })}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.version')}</dt><dd className="font-mono font-medium">v{provider.version}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.secret')}</dt><dd className="font-medium">{t(provider.secret_configured ? 'organizationConsole.settings.sso.configured' : 'organizationConsole.settings.sso.notConfigured')}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.fields.metadata')}</dt><dd className="font-medium">{t(provider.metadata_projection ? 'organizationConsole.settings.sso.configured' : 'organizationConsole.settings.sso.notConfigured')}</dd></div>
      </dl>
    </article>
  )
}

function SsoPanelLoading() {
  return <div className="grid gap-3 p-4" role="status"><Skeleton className="h-16" /><Skeleton className="h-16" /></div>
}

function SsoPanelState({ icon: Icon, title, body, action }: { icon: typeof ShieldBan; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-36 place-items-center p-6 text-center"><div className="max-w-md"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
