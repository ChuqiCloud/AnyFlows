import { Button, Chip, Switch } from '@heroui/react'
import { Pencil, Trash2, UsersRound, WalletCards } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'

type UserTableProps = {
  users: AdminUser[]
  groupNames: ReadonlyMap<number, string>
  togglingId?: number
  onDelete: (user: AdminUser) => void
  onEdit: (user: AdminUser) => void
  onToggle: (user: AdminUser) => void
  onWallet: (user: AdminUser) => void
}

const statusStyles = {
  enabled: 'bg-success/10 text-success',
  disabled: 'bg-surface-2 text-muted-foreground',
} as const

function IdentityBadges({ user }: { user: AdminUser }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      <Chip className={user.role === 'admin' ? 'bg-info/10 text-info' : 'bg-surface-2 text-muted-foreground'} size="sm" variant="flat">{t(`users.role.${user.role}`)}</Chip>
      <Chip className={cn(statusStyles[user.status])} size="sm" variant="flat">{t(`users.status.${user.status}`)}</Chip>
    </div>
  )
}

function GroupValue({ user, groupNames }: { user: AdminUser; groupNames: ReadonlyMap<number, string> }) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="truncate font-medium">{groupNames.get(user.default_group_id) ?? t('users.values.unknownGroup')}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">#{user.default_group_id}</div>
    </div>
  )
}

function QuotaValue({ user, formatQuota }: { user: AdminUser; formatQuota: (value: number) => string }) {
  const { t } = useTranslation()
  return (
    <div className="tabular-nums">
      <div>{t('users.values.totalQuota', { value: formatQuota(user.quota) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('users.values.quotaUsage', { used: formatQuota(user.used_quota), frozen: formatQuota(user.frozen_quota) })}</div>
    </div>
  )
}

function TrafficValue({ user }: { user: AdminUser }) {
  const { i18n, t } = useTranslation()
  const format = (value: number) => new Intl.NumberFormat(i18n.language).format(value)
  return (
    <div>
      <div className="tabular-nums">{t('users.values.requests', { value: format(user.request_count) })}</div>
      <div className="mt-1 flex flex-wrap gap-1">
        <Chip size="sm" variant="flat">{t('users.values.rpm', { value: user.rpm_limit === null ? t('users.values.unlimited') : format(user.rpm_limit) })}</Chip>
        <Chip size="sm" variant="flat">{t('users.values.concurrency', { value: user.concurrency === null ? t('users.values.unlimited') : format(user.concurrency) })}</Chip>
      </div>
    </div>
  )
}

function RowActions(props: {
  user: AdminUser
  toggling: boolean
  onDelete: (user: AdminUser) => void
  onEdit: (user: AdminUser) => void
  onToggle: (user: AdminUser) => void
  onWallet: (user: AdminUser) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <Switch isSelected={props.user.status === 'enabled'} isDisabled={props.toggling} aria-label={t(props.user.status === 'enabled' ? 'users.actions.disable' : 'users.actions.enable')} size="sm" onValueChange={() => props.onToggle(props.user)} />
      <SiteTooltip content={t('users.actions.wallet')}>
        <Button isIconOnly aria-label={t('users.actions.wallet')} className="size-10 md:size-8" size="sm" type="button" variant="light" onClick={() => props.onWallet(props.user)}><WalletCards className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
      <SiteTooltip content={t('users.actions.edit')}>
        <Button isIconOnly aria-label={t('users.actions.edit')} className="size-10 md:size-8" size="sm" type="button" variant="light" onClick={() => props.onEdit(props.user)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
      <SiteTooltip content={t('users.actions.delete')}>
        <Button isIconOnly aria-label={t('users.actions.delete')} className="size-10 text-muted-foreground hover:text-destructive md:size-8" size="sm" type="button" variant="light" onClick={() => props.onDelete(props.user)}><Trash2 className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
    </div>
  )
}

export function UserTable(props: UserTableProps) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  if (props.users.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><UsersRound className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('users.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('users.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr><th className="w-[23%] px-3 py-2 font-medium">{t('users.columns.user')}</th><th className="w-[13%] px-3 py-2 font-medium">{t('users.columns.group')}</th><th className="w-[19%] px-3 py-2 font-medium">{t('users.columns.quota')}</th><th className="w-[21%] px-3 py-2 font-medium">{t('users.columns.traffic')}</th><th className="w-[24%] px-3 py-2"><span className="sr-only">{t('users.columns.actions')}</span></th></tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {props.users.map((user) => (
              <tr key={user.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><div className="truncate font-medium">{user.username}</div><div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{user.email ?? t('users.values.noEmail')} · #{user.id}</div><div className="mt-1.5"><IdentityBadges user={user} /></div></td>
                <td className="px-3 py-2.5"><GroupValue user={user} groupNames={props.groupNames} /></td>
                <td className="px-3 py-2.5"><QuotaValue user={user} formatQuota={formatQuota} /></td>
                <td className="px-3 py-2.5"><TrafficValue user={user} /></td>
                <td className="px-2 py-2.5"><RowActions user={user} toggling={props.togglingId === user.id} onDelete={props.onDelete} onEdit={props.onEdit} onToggle={props.onToggle} onWallet={props.onWallet} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {props.users.map((user) => (
          <article key={user.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><div className="min-w-0"><h2 className="truncate text-sm font-semibold">{user.username}</h2><p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{user.email ?? t('users.values.noEmail')} · #{user.id}</p><div className="mt-2"><IdentityBadges user={user} /></div></div><RowActions user={user} toggling={props.togglingId === user.id} onDelete={props.onDelete} onEdit={props.onEdit} onToggle={props.onToggle} onWallet={props.onWallet} /></div>
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3"><GroupValue user={user} groupNames={props.groupNames} /><QuotaValue user={user} formatQuota={formatQuota} /></div>
            <div className="mt-3"><TrafficValue user={user} /></div>
          </article>
        ))}
      </div>
    </>
  )
}
