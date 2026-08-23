import type { ComponentProps } from 'react'
import { useEffect, useState } from 'react'
import { GitFork, KeyRound, Link2, LoaderCircle, Pencil, Power, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { updateCredentialStatusRequest } from './credential-form-model'
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
export function CredentialItem({ credential, parent, language, pending, active, onToggle, onEdit, onDelete, onRefresh }: CredentialItemProps) {
  const { t } = useTranslation()
  const [oauthOpen, setOAuthOpen] = useState(false)
  const writableKind = asWritableCredentialKind(credential.kind)
  const Icon = writableKind ? credentialKindIcons[writableKind] : KeyRound
  const runtimeState = credentialRuntimeState(credential)
  const coolingUntil = credentialCoolingUntil(credential)
  const shadow = isSparkShadowCredential(credential)
  const parentBlocked = shadow && sparkShadowParentBlocked(parent)
  const isOAuth = credential.kind === 'oauth' && !shadow
  const canToggle = !credential.oauth_token_pending
    && updateCredentialStatusRequest(credential, 'enabled') !== undefined

  useEffect(() => {
    if (!active) setOAuthOpen(false)
  }, [active, credential.id])

  return (
    <article className="py-3.5">
      <div className="grid grid-cols-[2.25rem_minmax(0,1fr)] items-start gap-x-3 gap-y-2 sm:grid-cols-[2.25rem_minmax(0,1fr)_auto]">
        <div className="grid size-9 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Icon className="size-4" aria-hidden="true" /></div>
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-1.5">
            <h3 className="text-xs font-semibold">#{credential.id}</h3>
            {shadow ? <Badge className="gap-1 border-transparent bg-info/10 text-info"><GitFork className="size-3" aria-hidden="true" />{t('credentials.spark.badge')}</Badge> : null}
            <Badge>{t(`credentials.kind.${credential.kind}`, { defaultValue: credential.kind })}</Badge>
            <Badge className={cn('border-transparent', stateStyles[runtimeState])}>{t(`credentials.runtime.${runtimeState}`)}</Badge>
            {parentBlocked ? <Badge className="border-transparent bg-destructive/10 text-destructive">{t('credentials.runtime.parentBlocked')}</Badge> : null}
            {isOAuth ? <Badge className="max-w-48 truncate">{credential.oauth_provider ?? t('credentials.oauth.unbound')}</Badge> : null}
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
          {isOAuth ? <IconAction label={t(credential.oauth_token_pending ? 'credentials.actions.connect' : credential.oauth_provider ? 'credentials.actions.reauthorize' : 'credentials.actions.connect')} aria-expanded={oauthOpen} onClick={() => setOAuthOpen((current) => !current)}><Link2 aria-hidden="true" /></IconAction> : null}
          <IconAction label={t(credential.status === 'enabled' ? 'credentials.actions.disable' : 'credentials.actions.enable')} disabled={pending || oauthOpen || !canToggle} onClick={() => onToggle(credential)}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Power aria-hidden="true" />}</IconAction>
          <IconAction label={t('credentials.actions.edit')} disabled={pending || oauthOpen || !writableKind} onClick={() => onEdit(credential)}><Pencil aria-hidden="true" /></IconAction>
          <IconAction label={t('credentials.actions.delete')} className="hover:text-destructive" disabled={oauthOpen} onClick={() => onDelete(credential)}><Trash2 aria-hidden="true" /></IconAction>
        </div>
      </div>
      {isOAuth && oauthOpen ? <CredentialOAuthConnection active={active && oauthOpen} channelId={credential.channel_id} credential={credential} onRefresh={onRefresh} /> : null}
    </article>
  )
}

function IconAction({ label, className, children, ...props }: ComponentProps<typeof Button> & { label: string }) {
  return <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className={className} aria-label={label} {...props}>{children}</Button></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
}

function formatTimestamp(timestamp: number, language: string) {
  return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(timestamp * 1_000)
}
