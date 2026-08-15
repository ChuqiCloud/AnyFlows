import { KeyRound, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { UserToken } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'

type ApiKeyTableProps = {
  tokens: UserToken[]
  togglingId?: number
  onDelete: (token: UserToken) => void
  onEdit: (token: UserToken) => void
  onToggle: (token: UserToken) => void
}

function isExpired(token: UserToken) {
  return token.expired_at !== null && token.expired_at <= Math.floor(Date.now() / 1000)
}

export function isApiKeyAvailable(token: UserToken) {
  return token.status === 'enabled' && !isExpired(token)
}

function StatusBadge({ token }: { token: UserToken }) {
  const { t } = useTranslation()
  const expired = isExpired(token)
  return (
    <Badge className={cn(
      'border-transparent',
      expired ? 'bg-warning/10 text-warning' : token.status === 'enabled' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground',
    )}>
      {t(expired ? 'apiKeys.status.expired' : `apiKeys.status.${token.status}`)}
    </Badge>
  )
}

function QuotaValue({ token }: { token: UserToken }) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  return (
    <>
      <div>{token.unlimited_quota ? t('apiKeys.values.unlimited') : t('apiKeys.values.remaining', { value: formatQuota(token.remain_quota) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('apiKeys.values.usedQuota', { value: formatQuota(token.used_quota) })}</div>
    </>
  )
}

function PolicyValue({ token }: { token: UserToken }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      <Badge className="bg-surface-2 text-muted-foreground">
        {token.model_limits ? t('apiKeys.values.models', { count: token.model_limits.length }) : t('apiKeys.values.allModels')}
      </Badge>
      <Badge className="bg-surface-2 text-muted-foreground">
        {token.allow_ips ? t('apiKeys.values.ips', { count: token.allow_ips.length }) : t('apiKeys.values.allIps')}
      </Badge>
    </div>
  )
}

function ValidityValue({ token }: { token: UserToken }) {
  const { i18n, t } = useTranslation()
  const expiry = token.expired_at === null
    ? t('apiKeys.values.neverExpires')
    : new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium' }).format(token.expired_at * 1000)
  const updated = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'short', timeStyle: 'short' })
    .format(token.updated_at * 1000)
  return (
    <>
      <div>{expiry}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('apiKeys.values.updatedAt', { value: updated })}</div>
    </>
  )
}

function RowActions(props: {
  token: UserToken
  toggling: boolean
  onDelete: (token: UserToken) => void
  onEdit: (token: UserToken) => void
  onToggle: (token: UserToken) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <Switch
        checked={props.token.status === 'enabled'}
        disabled={props.toggling}
        aria-label={t(props.token.status === 'enabled' ? 'apiKeys.actions.disable' : 'apiKeys.actions.enable')}
        onCheckedChange={() => props.onToggle(props.token)}
      />
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 md:size-8" aria-label={t('apiKeys.actions.edit')} onClick={() => props.onEdit(props.token)}><Pencil aria-hidden="true" /></Button>
        </TooltipTrigger>
        <TooltipContent>{t('apiKeys.actions.edit')}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 text-muted-foreground hover:text-destructive md:size-8" aria-label={t('apiKeys.actions.delete')} onClick={() => props.onDelete(props.token)}><Trash2 aria-hidden="true" /></Button>
        </TooltipTrigger>
        <TooltipContent>{t('apiKeys.actions.delete')}</TooltipContent>
      </Tooltip>
    </div>
  )
}

export function ApiKeyTable(props: ApiKeyTableProps) {
  const { t } = useTranslation()
  if (props.tokens.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><KeyRound className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('apiKeys.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('apiKeys.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr><th className="w-[26%] px-3 py-2 font-medium">{t('apiKeys.columns.key')}</th><th className="w-[20%] px-3 py-2 font-medium">{t('apiKeys.columns.quota')}</th><th className="w-[20%] px-3 py-2 font-medium">{t('apiKeys.columns.policy')}</th><th className="w-[20%] px-3 py-2 font-medium">{t('apiKeys.columns.validity')}</th><th className="w-[14%] px-3 py-2"><span className="sr-only">{t('apiKeys.columns.actions')}</span></th></tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {props.tokens.map((token) => (
              <tr key={token.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><div className="flex items-center gap-2"><span className="truncate font-medium">{token.name}</span><StatusBadge token={token} /></div><div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}... <span aria-hidden="true">#</span>{token.id}</div></td>
                <td className="px-3 py-2.5 tabular-nums"><QuotaValue token={token} /></td>
                <td className="px-3 py-2.5"><PolicyValue token={token} /></td>
                <td className="px-3 py-2.5 tabular-nums"><ValidityValue token={token} /></td>
                <td className="px-2 py-2.5"><RowActions token={token} toggling={props.togglingId === token.id} onDelete={props.onDelete} onEdit={props.onEdit} onToggle={props.onToggle} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {props.tokens.map((token) => (
          <article key={token.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><h2 className="truncate text-sm font-semibold">{token.name}</h2><StatusBadge token={token} /></div><p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{token.key_prefix}... <span aria-hidden="true">#</span>{token.id}</p></div><RowActions token={token} toggling={props.togglingId === token.id} onDelete={props.onDelete} onEdit={props.onEdit} onToggle={props.onToggle} /></div>
            <dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              <div><dt className="text-muted-foreground">{t('apiKeys.columns.quota')}</dt><dd className="mt-1 tabular-nums"><QuotaValue token={token} /></dd></div>
              <div><dt className="text-muted-foreground">{t('apiKeys.columns.validity')}</dt><dd className="mt-1 tabular-nums"><ValidityValue token={token} /></dd></div>
              <div className="col-span-2"><dt className="text-muted-foreground">{t('apiKeys.columns.policy')}</dt><dd className="mt-1"><PolicyValue token={token} /></dd></div>
            </dl>
          </article>
        ))}
      </div>
    </>
  )
}
