import type { ComponentProps } from 'react'
import { useEffect, useState } from 'react'
import { Gauge, GitFork, KeyRound, Link2, LoaderCircle, Pencil, Power, RefreshCw, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button, Checkbox, Chip } from '@heroui/react'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import type { CredentialHealthSnapshot } from './credential-api'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'
import { updateCredentialStatusRequest } from './credential-form-model'
import { useAdminCredentialUsage } from './credential-api'
import { credentialKindIcons } from './credential-kind-picker'
import {
  asWritableCredentialKind,
  credentialCoolingUntil,
  credentialRuntimeState,
  isSparkShadowCredential,
  sparkShadowParentBlocked,
} from './credential-model'
import { CredentialOAuthConnection } from './credential-oauth-connection'

const stateStyles = {
  available: 'bg-success/10 text-success',
  authorizationPending: 'bg-info/10 text-info',
  autoDisabled: 'bg-destructive/10 text-destructive',
  cooling: 'bg-warning/10 text-warning',
  disabled: 'bg-surface-2 text-muted-foreground',
  paused: 'bg-surface-2 text-muted-foreground',
  pending: 'bg-info/10 text-info',
} as const

type CredentialItemProps = {
  selected?: boolean
  onSelect?: () => void
  health?: CredentialHealthSnapshot
  channelType: string
  credential: AdminCredential
  parent?: AdminCredential
  language: string
  pending: boolean
  active: boolean
  onToggle: (credential: AdminCredential) => void
  onEdit: (credential: AdminCredential) => void
  onDelete: (credential: AdminCredential) => void
  onRefresh: () => Promise<unknown>
}

/** 以真实持久化与冷却事实呈现账号状态，不接触任何密文明文。 */
export function CredentialItem({ selected, onSelect, health, channelType, credential, parent, language, pending, active, onToggle, onEdit, onDelete, onRefresh }: CredentialItemProps) {
  const { t } = useTranslation()
  const [oauthOpen, setOAuthOpen] = useState(false)
  const [usageOpen, setUsageOpen] = useState(false)
  const writableKind = asWritableCredentialKind(credential.kind)
  const Icon = writableKind ? credentialKindIcons[writableKind] : KeyRound
  const runtimeState = credentialRuntimeState(credential)
  const coolingUntil = credentialCoolingUntil(credential)
  const shadow = isSparkShadowCredential(credential)
  const parentBlocked = shadow && sparkShadowParentBlocked(parent)
  const isOAuth = credential.kind === 'oauth' && !shadow
  const canQueryUsage = channelType === 'openai' && isOAuth && credential.oauth_provider === 'codex' && !credential.oauth_token_pending
  const usageExpanded = usageOpen && active && canQueryUsage
  const canToggle = !credential.oauth_token_pending
    && updateCredentialStatusRequest(credential, 'enabled') !== undefined

  useEffect(() => {
    if (!active) {
      setOAuthOpen(false)
    }
    if (!active || !canQueryUsage) {
      setUsageOpen(false)
    }
  }, [active, canQueryUsage])

  function toggleUsage() {
    setUsageOpen((current) => !current)
  }

  return (
    <article className="py-3.5">
      <div className="grid grid-cols-[2.25rem_minmax(0,1fr)] items-start gap-x-3 gap-y-2 sm:grid-cols-[2.25rem_minmax(0,1fr)_auto]">
        <div className="grid size-9 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Icon className="size-4" aria-hidden="true" /></div>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            {onSelect ? <Checkbox aria-label={t('credentials.batch.selectOne', { defaultValue: '选择凭据 #{{id}}', id: credential.id })} isSelected={Boolean(selected)} size="sm" onValueChange={() => onSelect()} /> : null}
            <h3 className="text-xs font-semibold">#{credential.id}</h3>
            {shadow ? <Chip className="gap-1 bg-info/10 text-info" size="sm" variant="flat"><GitFork className="size-3" aria-hidden="true" />{t('credentials.spark.badge')}</Chip> : null}
            <Chip size="sm" variant="flat">{t(`credentials.kind.${credential.kind}`, { defaultValue: credential.kind })}</Chip>
            <Chip className={cn(stateStyles[runtimeState])} size="sm" variant="flat">{t(`credentials.runtime.${runtimeState}`)}</Chip>
            {health && health.weight_multiplier_micros < 1_000_000 ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('credentials.health.reduced', { percent: Math.round(health.weight_multiplier_micros / 10_000) })}</Chip> : null}
            {parentBlocked ? <Chip className="bg-destructive/10 text-destructive" size="sm" variant="flat">{t('credentials.runtime.parentBlocked')}</Chip> : null}
            {isOAuth ? <Chip className="max-w-48 truncate" size="sm" variant="flat">{credential.oauth_provider ?? t('credentials.oauth.unbound')}</Chip> : null}
          </div>
          <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[0.6875rem] text-muted-foreground tabular-nums">
            <span>{t('credentials.item.priorityWeight', { priority: credential.priority, weight: credential.weight })}</span>
            <span>{shadow
              ? t('credentials.item.inheritedConcurrency', { value: parent?.concurrency ?? t('credentials.values.unlimited') })
              : t('credentials.item.concurrency', { value: credential.concurrency ?? t('credentials.values.unlimited') })}</span>
            {shadow ? <span>{t('credentials.item.sharedParent', { id: credential.parent_id ?? '?' })}</span> : null}
            {shadow ? <span>{parent?.proxy_id
              ? t('credentials.item.inheritedProxy', { id: parent.proxy_id })
              : t('credentials.item.inheritedGlobalProxy')}</span> : credential.proxy_id ? <span>{t('credentials.item.proxy', { id: credential.proxy_id })}</span> : null}
          </div>
          {health && health.weight_multiplier_micros < 1_000_000 ? <p className="mt-1.5 text-[0.6875rem] text-warning">{health.cooling ? t('credentials.health.cooling') : t('credentials.health.reducedDescription', { percent: Math.round(health.weight_multiplier_micros / 10_000) })}</p> : null}
          <p className="mt-1.5 text-[0.6875rem] text-muted-foreground">
            {credential.oauth_token_pending
              ? t('credentials.item.authorizationRequired')
              : coolingUntil && coolingUntil > Date.now() / 1_000
              ? t('credentials.item.coolingUntil', { value: formatTimestamp(coolingUntil, language) })
              : credential.last_used_at === null
                ? t('credentials.item.neverUsed')
                : t('credentials.item.lastUsed', { value: formatTimestamp(credential.last_used_at, language) })}
          </p>
        </div>
        <div className="col-start-2 flex items-center justify-end gap-1 sm:col-start-3">
          {canQueryUsage ? <IconAction label={t('credentials.usage.action')} aria-expanded={usageExpanded} onClick={toggleUsage}><Gauge className="size-3.5" aria-hidden="true" /></IconAction> : null}
          {isOAuth ? <IconAction label={t(credential.oauth_token_pending ? 'credentials.actions.connect' : credential.oauth_provider ? 'credentials.actions.reauthorize' : 'credentials.actions.connect')} aria-expanded={oauthOpen} onClick={() => setOAuthOpen((current) => !current)}><Link2 className="size-3.5" aria-hidden="true" /></IconAction> : null}
          <IconAction label={t(credential.status === 'enabled' ? 'credentials.actions.disable' : 'credentials.actions.enable')} disabled={pending || oauthOpen || !canToggle} onClick={() => onToggle(credential)}>{pending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Power className="size-3.5" aria-hidden="true" />}</IconAction>
          <IconAction label={t('credentials.actions.edit')} disabled={pending || oauthOpen || !writableKind} onClick={() => onEdit(credential)}><Pencil className="size-3.5" aria-hidden="true" /></IconAction>
          <IconAction label={t('credentials.actions.delete')} className="hover:text-destructive" disabled={oauthOpen} onClick={() => onDelete(credential)}><Trash2 className="size-3.5" aria-hidden="true" /></IconAction>
        </div>
      </div>
      {usageExpanded ? <CredentialUsagePanel channelId={credential.channel_id} credentialId={credential.id} language={language} /> : null}
      {isOAuth && oauthOpen ? <CredentialOAuthConnection active={active && oauthOpen} channelType={channelType} channelId={credential.channel_id} credential={credential} onDelete={onDelete} onRefresh={onRefresh} /> : null}
    </article>
  )
}

function CredentialUsagePanel({ channelId, credentialId, language }: { channelId: number; credentialId: number; language: string }) {
  const { t } = useTranslation()
  const usageQuery = useAdminCredentialUsage(channelId, credentialId)

  return <div className="mt-2 flex items-start justify-between gap-2 border-t border-border pt-2 text-xs text-muted-foreground">
    <div className="min-w-0 flex-1" role="status" aria-live="polite">
      {usageQuery.isPending && !usageQuery.data ? t('credentials.usage.loading') : usageQuery.data?.status === 'available' ? <div className="flex flex-wrap gap-x-5 gap-y-1">
        {usageQuery.data.windows.map((window, index) => <span key={`${window.window_seconds}-${index}`}>{t('credentials.usage.window', { hours: window.window_seconds / 3600, percent: window.used_percent.toFixed(1) })}{window.reset_at ? ` · ${t('credentials.usage.reset', { value: formatTimestamp(window.reset_at, language) })}` : ''}</span>)}
        {usageQuery.data.credits_balance !== null ? <span>{t('credentials.usage.credits', { value: usageQuery.data.credits_balance })}</span> : null}
        {usageQuery.data.fetched_at ? <span>{t('credentials.usage.fetched', { value: formatTimestamp(usageQuery.data.fetched_at, language) })}</span> : null}
      </div> : t(`credentials.usage.${usageQuery.data?.status ?? 'unavailable'}`)}
    </div>
    <IconAction label={t('credentials.actions.refresh')} disabled={usageQuery.isFetching} onClick={() => void usageQuery.refetch()}>
      <RefreshCw className={`size-3.5 ${usageQuery.isFetching ? 'animate-spin' : ''}`} aria-hidden="true" />
    </IconAction>
  </div>
}

function IconAction({ label, className, children, ...props }: ComponentProps<typeof Button> & { label: string }) {
  return <SiteTooltip content={label}><Button type="button" isIconOnly size="sm" variant="light" className={className} aria-label={label} {...props}>{children}</Button></SiteTooltip>
}

function formatTimestamp(timestamp: number, language: string) {
  return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(timestamp * 1_000)
}
