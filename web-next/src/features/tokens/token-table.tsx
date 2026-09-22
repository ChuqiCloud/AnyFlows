import { Button, Chip } from '@heroui/react'
import { KeyRound, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { AdminToken } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { SiteTooltip } from '@/shared/components/site-tooltip'

type TokenTableProps = {
  tokens: AdminToken[]
  onDelete: (token: AdminToken) => void
  onEdit: (token: AdminToken) => void
}

const TOKEN_WINDOWS = [
  { label: '5h', limit: 'rate_limit_5h', usage: 'usage_5h', startedAt: 'window_5h_start', duration: 5 * 60 * 60 },
  { label: '1d', limit: 'rate_limit_1d', usage: 'usage_1d', startedAt: 'window_1d_start', duration: 24 * 60 * 60 },
  { label: '7d', limit: 'rate_limit_7d', usage: 'usage_7d', startedAt: 'window_7d_start', duration: 7 * 24 * 60 * 60 },
] as const

function isExpired(token: AdminToken) {
  return token.expired_at !== null && token.expired_at <= Math.floor(Date.now() / 1000)
}

function StatusBadge({ token }: { token: AdminToken }) {
  const { t } = useTranslation()
  const expired = isExpired(token)
  return (
    <Chip className={cn(
      expired ? 'bg-warning/10 text-warning' : token.status === 'enabled' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground',
    )} size="sm" variant="flat">
      {t(expired ? 'tokens.status.expired' : `tokens.status.${token.status}`)}
    </Chip>
  )
}

function RowActions({ token, onDelete, onEdit }: {
  token: AdminToken
  onDelete: (token: AdminToken) => void
  onEdit: (token: AdminToken) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <SiteTooltip content={t('tokens.actions.edit')}>
        <Button isIconOnly type="button" size="sm" variant="light" className="size-10 md:size-8" aria-label={t('tokens.actions.edit')} onClick={() => onEdit(token)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
      <SiteTooltip content={t('tokens.actions.delete')}>
        <Button isIconOnly type="button" size="sm" variant="light" className="size-10 text-muted-foreground hover:text-destructive md:size-8" aria-label={t('tokens.actions.delete')} onClick={() => onDelete(token)}><Trash2 className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
    </div>
  )
}

function QuotaValue({ token }: { token: AdminToken }) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const resetFormat = new Intl.DateTimeFormat(i18n.language, {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
  const now = Math.floor(Date.now() / 1000)
  return (
    <>
      <div>{token.unlimited_quota ? t('tokens.values.unlimited') : t('tokens.values.remaining', { value: formatQuota(token.remain_quota) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('tokens.values.usedQuota', { value: formatQuota(token.used_quota) })}</div>
      <div className="mt-2 grid gap-1.5 border-t border-[var(--hairline)] pt-2">
        {TOKEN_WINDOWS.map((window) => {
          const limit = token[window.limit]
          const usage = token[window.usage]
          const resetAt = token[window.startedAt] + window.duration
          return (
            <div key={window.label} className="grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-1.5 text-[0.6875rem] leading-4">
              <Chip className="h-4 rounded px-1 text-[0.625rem] font-medium text-muted-foreground" size="sm" variant="flat">{window.label}</Chip>
              <div className="min-w-0">
                <div className="truncate font-medium tabular-nums">
                  {limit === null
                    ? t('tokens.values.windowUsageUnlimited', { used: formatQuota(usage) })
                    : t('tokens.values.windowUsage', { used: formatQuota(usage), limit: formatQuota(limit) })}
                </div>
                <time
                  className="block truncate text-[0.625rem] text-muted-foreground"
                  dateTime={new Date(resetAt * 1000).toISOString()}
                  title={resetFormat.format(resetAt * 1000)}
                >
                  {resetAt <= now
                    ? t('tokens.values.windowResetPending')
                    : t('tokens.values.windowResetsAt', { value: resetFormat.format(resetAt * 1000) })}
                </time>
              </div>
            </div>
          )
        })}
      </div>
    </>
  )
}

function PolicyValue({ token }: { token: AdminToken }) {
  const { t } = useTranslation()
  return (
    <>
      <div>{token.model_limits ? t('tokens.values.models', { count: token.model_limits.length }) : t('tokens.values.allModels')}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{token.allow_ips ? t('tokens.values.ips', { count: token.allow_ips.length }) : t('tokens.values.allIps')}</div>
    </>
  )
}

function ValidityValue({ token }: { token: AdminToken }) {
  const { i18n, t } = useTranslation()
  const expiry = token.expired_at === null
    ? t('tokens.values.neverExpires')
    : new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium' }).format(token.expired_at * 1000)
  return (
    <>
      <div>{expiry}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {token.max_requests === null
          ? t('tokens.values.requestsUsed', { value: token.used_requests })
          : t('tokens.values.requestsRatio', { used: token.used_requests, total: token.max_requests })}
      </div>
    </>
  )
}

export function TokenTable({ tokens, onDelete, onEdit }: TokenTableProps) {
  const { t } = useTranslation()
  if (tokens.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><KeyRound className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('tokens.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('tokens.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr><th className="w-[24%] px-3 py-2 font-medium">{t('tokens.columns.token')}</th><th className="w-[13%] px-3 py-2 font-medium">{t('tokens.columns.owner')}</th><th className="w-[23%] px-3 py-2 font-medium">{t('tokens.columns.quota')}</th><th className="w-[16%] px-3 py-2 font-medium">{t('tokens.columns.policy')}</th><th className="w-[16%] px-3 py-2 font-medium">{t('tokens.columns.validity')}</th><th className="w-[8%] px-3 py-2"><span className="sr-only">{t('tokens.columns.actions')}</span></th></tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {tokens.map((token) => (
              <tr key={token.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><div className="flex items-center gap-2"><span className="truncate font-medium">{token.name}</span><StatusBadge token={token} /></div><div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}… · #{token.id}</div></td>
                <td className="px-3 py-2.5"><div>{t('tokens.values.user', { id: token.user_id })}</div><div className="mt-1 text-[0.6875rem] text-muted-foreground">{token.group_id ? t('tokens.values.group', { id: token.group_id }) : t('tokens.values.defaultGroup')}</div></td>
                <td className="px-3 py-2.5 tabular-nums"><QuotaValue token={token} /></td>
                <td className="px-3 py-2.5"><PolicyValue token={token} /></td>
                <td className="px-3 py-2.5 tabular-nums"><ValidityValue token={token} /></td>
                <td className="px-2 py-2.5"><RowActions token={token} onDelete={onDelete} onEdit={onEdit} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {tokens.map((token) => (
          <article key={token.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><h2 className="truncate text-sm font-semibold">{token.name}</h2><StatusBadge token={token} /></div><p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}… · #{token.id}</p></div><RowActions token={token} onDelete={onDelete} onEdit={onEdit} /></div>
            <dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              <div><dt className="text-muted-foreground">{t('tokens.columns.owner')}</dt><dd className="mt-1">{t('tokens.values.user', { id: token.user_id })}</dd></div>
              <div><dt className="text-muted-foreground">{t('tokens.columns.quota')}</dt><dd className="mt-1 tabular-nums"><QuotaValue token={token} /></dd></div>
              <div><dt className="text-muted-foreground">{t('tokens.columns.policy')}</dt><dd className="mt-1"><PolicyValue token={token} /></dd></div>
              <div><dt className="text-muted-foreground">{t('tokens.columns.validity')}</dt><dd className="mt-1 tabular-nums"><ValidityValue token={token} /></dd></div>
            </dl>
          </article>
        ))}
      </div>
    </>
  )
}
