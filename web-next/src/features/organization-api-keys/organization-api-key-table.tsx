import { Button, Chip } from '@heroui/react'
import { KeyRound, Pencil, Power } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { OrganizationTokenResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'

type OrganizationApiKeyTableProps = {
  mode: 'directory' | 'self'
  tokens: OrganizationTokenResponse[]
  disablingId?: number
  onDisable: (token: OrganizationTokenResponse) => void
  onEdit: (token: OrganizationTokenResponse) => void
}

const TOKEN_WINDOWS = [
  { label: '5h', limit: 'rate_limit_5h', usage: 'usage_5h' },
  { label: '1d', limit: 'rate_limit_1d', usage: 'usage_1d' },
  { label: '7d', limit: 'rate_limit_7d', usage: 'usage_7d' },
] as const

function isExpired(token: OrganizationTokenResponse) {
  return token.expired_at !== null && token.expired_at <= Math.floor(Date.now() / 1_000)
}

export function isOrganizationApiKeyAvailable(token: OrganizationTokenResponse) {
  return token.status === 'enabled' && !isExpired(token)
}

function StatusBadge({ token }: { token: OrganizationTokenResponse }) {
  const { t } = useTranslation()
  const expired = isExpired(token)
  return (
    <Chip className={cn(
      expired ? 'bg-warning/10 text-warning' : token.status === 'enabled' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground',
    )} size="sm" variant="flat">
      {t(expired ? 'apiKeys.status.expired' : `apiKeys.status.${token.status}`)}
    </Chip>
  )
}

function QuotaValue({ token }: { token: OrganizationTokenResponse }) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  return (
    <>
      <div>{token.unlimited_quota ? t('apiKeys.values.unlimited') : t('apiKeys.values.remaining', { value: formatQuota(token.remain_quota) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('apiKeys.values.usedQuota', { value: formatQuota(token.used_quota) })}</div>
    </>
  )
}

function PolicyValue({ token }: { token: OrganizationTokenResponse }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">
        {token.model_limits ? t('apiKeys.values.models', { count: token.model_limits.length }) : t('apiKeys.values.allModels')}
      </Chip>
      <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">
        {token.allow_ips ? t('apiKeys.values.ips', { count: token.allow_ips.length }) : t('apiKeys.values.allIps')}
      </Chip>
    </div>
  )
}

function LimitValue({ token }: { token: OrganizationTokenResponse }) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  return (
    <div className="grid gap-1.5">
      <div className="flex flex-wrap gap-1">
        {TOKEN_WINDOWS.map((window) => {
          const limit = token[window.limit]
          const usage = token[window.usage]
          return (
            <Chip key={window.label} className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">
              {limit === null
                ? t('organizationApiKeys.values.windowUnlimited', { window: window.label, used: formatQuota(usage) })
                : t('organizationApiKeys.values.windowUsage', { window: window.label, used: formatQuota(usage), limit: formatQuota(limit) })}
            </Chip>
          )
        })}
      </div>
      <div className="text-[0.6875rem] text-muted-foreground">
        {token.max_requests === null
          ? t('organizationApiKeys.values.requestsUsed', { used: token.used_requests })
          : t('organizationApiKeys.values.requestsRatio', { used: token.used_requests, limit: token.max_requests })}
      </div>
    </div>
  )
}

function ValidityValue({ token }: { token: OrganizationTokenResponse }) {
  const { i18n, t } = useTranslation()
  const expiry = token.expired_at === null
    ? t('apiKeys.values.neverExpires')
    : new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium' }).format(token.expired_at * 1_000)
  const updated = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'short', timeStyle: 'short' })
    .format(token.updated_at * 1_000)
  return (
    <>
      <div>{expiry}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('apiKeys.values.updatedAt', { value: updated })}</div>
    </>
  )
}

function OwnerValue({ token }: { token: OrganizationTokenResponse }) {
  const { t } = useTranslation()
  return (
    <>
      <div>{t('organizationApiKeys.values.owner', { id: token.owner_user_id })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {token.team_id === null
          ? t('organizationApiKeys.values.noTeam')
          : t('organizationApiKeys.values.team', { id: token.team_id })}
      </div>
    </>
  )
}

function RowActions(props: {
  mode: OrganizationApiKeyTableProps['mode']
  token: OrganizationTokenResponse
  disabling: boolean
  onDisable: (token: OrganizationTokenResponse) => void
  onEdit: (token: OrganizationTokenResponse) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      {props.mode === 'self' ? (
        <SiteTooltip content={t('apiKeys.actions.edit')}>
          <Button isIconOnly aria-label={t('apiKeys.actions.edit')} className="size-10 md:size-8" size="sm" type="button" variant="light" onClick={() => props.onEdit(props.token)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
        </SiteTooltip>
      ) : null}
      {props.token.status === 'enabled' ? (
        <SiteTooltip content={t('organizationApiKeys.actions.disable')}>
          <Button isIconOnly aria-label={t('organizationApiKeys.actions.disable')} className="size-10 text-muted-foreground hover:text-warning md:size-8" isDisabled={props.disabling} size="sm" type="button" variant="light" onClick={() => props.onDisable(props.token)}><Power className="size-3.5" aria-hidden="true" /></Button>
        </SiteTooltip>
      ) : null}
    </div>
  )
}

export function OrganizationApiKeyTable(props: OrganizationApiKeyTableProps) {
  const { t } = useTranslation()
  if (props.tokens.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><KeyRound className="size-4" aria-hidden="true" /></div>
          <h3 className="mt-3 text-sm font-semibold">{t(`organizationApiKeys.empty.${props.mode}Title`)}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(`organizationApiKeys.empty.${props.mode}Body`)}</p>
        </div>
      </div>
    )
  }

  const directory = props.mode === 'directory'
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 lg:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className={cn('px-3 py-2 font-medium', directory ? 'w-[18%]' : 'w-[24%]')}>{t('organizationApiKeys.columns.key')}</th>
              {directory ? <th className="w-[12%] px-3 py-2 font-medium">{t('organizationApiKeys.columns.owner')}</th> : null}
              <th className={cn('px-3 py-2 font-medium', directory ? 'w-[14%]' : 'w-[15%]')}>{t('organizationApiKeys.columns.quota')}</th>
              <th className={cn('px-3 py-2 font-medium', directory ? 'w-[13%]' : 'w-[15%]')}>{t('organizationApiKeys.columns.policy')}</th>
              <th className={cn('px-3 py-2 font-medium', directory ? 'w-[20%]' : 'w-[22%]')}>{t('organizationApiKeys.columns.limits')}</th>
              <th className={cn('px-3 py-2 font-medium', directory ? 'w-[16%]' : 'w-[17%]')}>{t('organizationApiKeys.columns.validity')}</th>
              <th className="w-[7%] px-3 py-2"><span className="sr-only">{t('organizationApiKeys.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {props.tokens.map((token) => (
              <tr key={token.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><div className="flex min-w-0 items-center gap-2"><span className="truncate font-medium">{token.name}</span><StatusBadge token={token} /></div><div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}... <span aria-hidden="true">#</span>{token.id}</div></td>
                {directory ? <td className="px-3 py-2.5"><OwnerValue token={token} /></td> : null}
                <td className="px-3 py-2.5 tabular-nums"><QuotaValue token={token} /></td>
                <td className="px-3 py-2.5"><PolicyValue token={token} /></td>
                <td className="px-3 py-2.5"><LimitValue token={token} /></td>
                <td className="px-3 py-2.5 tabular-nums"><ValidityValue token={token} /></td>
                <td className="px-2 py-2.5"><RowActions mode={props.mode} token={token} disabling={props.disablingId === token.id} onDisable={props.onDisable} onEdit={props.onEdit} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 lg:hidden">
        {props.tokens.map((token) => (
          <article key={token.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0"><div className="flex items-center gap-2"><h3 className="truncate text-sm font-semibold">{token.name}</h3><StatusBadge token={token} /></div><p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}... <span aria-hidden="true">#</span>{token.id}</p></div>
              <RowActions mode={props.mode} token={token} disabling={props.disablingId === token.id} onDisable={props.onDisable} onEdit={props.onEdit} />
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              {directory ? <div><dt className="text-muted-foreground">{t('organizationApiKeys.columns.owner')}</dt><dd className="mt-1"><OwnerValue token={token} /></dd></div> : null}
              <div><dt className="text-muted-foreground">{t('organizationApiKeys.columns.quota')}</dt><dd className="mt-1 tabular-nums"><QuotaValue token={token} /></dd></div>
              <div><dt className="text-muted-foreground">{t('organizationApiKeys.columns.validity')}</dt><dd className="mt-1 tabular-nums"><ValidityValue token={token} /></dd></div>
              <div className="col-span-2"><dt className="text-muted-foreground">{t('organizationApiKeys.columns.policy')}</dt><dd className="mt-1"><PolicyValue token={token} /></dd></div>
              <div className="col-span-2"><dt className="text-muted-foreground">{t('organizationApiKeys.columns.limits')}</dt><dd className="mt-1"><LimitValue token={token} /></dd></div>
            </dl>
          </article>
        ))}
      </div>
    </>
  )
}
