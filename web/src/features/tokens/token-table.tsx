import { KeyRound, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminToken } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type TokenTableProps = {
  tokens: AdminToken[]
  onDelete: (token: AdminToken) => void
  onEdit: (token: AdminToken) => void
}

function isExpired(token: AdminToken) {
  return token.expired_at !== null && token.expired_at <= Math.floor(Date.now() / 1000)
}

function StatusBadge({ token }: { token: AdminToken }) {
  const { t } = useTranslation()
  const expired = isExpired(token)
  return (
    <Badge className={cn(
      'border-transparent',
      expired ? 'bg-warning/10 text-warning' : token.status === 'enabled' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground',
    )}>
      {t(expired ? 'tokens.status.expired' : `tokens.status.${token.status}`)}
    </Badge>
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
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 md:size-8" aria-label={t('tokens.actions.edit')} onClick={() => onEdit(token)}><Pencil aria-hidden="true" /></Button>
        </TooltipTrigger>
        <TooltipContent>{t('tokens.actions.edit')}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 text-muted-foreground hover:text-destructive md:size-8" aria-label={t('tokens.actions.delete')} onClick={() => onDelete(token)}><Trash2 aria-hidden="true" /></Button>
        </TooltipTrigger>
        <TooltipContent>{t('tokens.actions.delete')}</TooltipContent>
      </Tooltip>
    </div>
  )
}

function QuotaValue({ token }: { token: AdminToken }) {
  const { i18n, t } = useTranslation()
  const format = (value: number) => new Intl.NumberFormat(i18n.language).format(value)
  return (
    <>
      <div>{token.unlimited_quota ? t('tokens.values.unlimited') : t('tokens.values.remaining', { value: format(token.remain_quota) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('tokens.values.usedQuota', { value: format(token.used_quota) })}</div>
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
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
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
            <tr><th className="w-[27%] px-3 py-2 font-medium">{t('tokens.columns.token')}</th><th className="w-[14%] px-3 py-2 font-medium">{t('tokens.columns.owner')}</th><th className="w-[18%] px-3 py-2 font-medium">{t('tokens.columns.quota')}</th><th className="w-[17%] px-3 py-2 font-medium">{t('tokens.columns.policy')}</th><th className="w-[16%] px-3 py-2 font-medium">{t('tokens.columns.validity')}</th><th className="w-[8%] px-3 py-2"><span className="sr-only">{t('tokens.columns.actions')}</span></th></tr>
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
          <article key={token.id} className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
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
