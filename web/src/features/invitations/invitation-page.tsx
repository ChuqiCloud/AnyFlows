import {
  Check,
  Coins,
  Copy,
  History,
  Link2,
  RefreshCw,
  ShieldCheck,
  TicketCheck,
  UsersRound,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { UserInvitationSummaryResponse } from '@/lib/api/generated/types.gen'
import { useUserInvitations } from './invitation-api'

type CopyTarget = 'code' | 'link'

/** 展示当前用户自己的邀请码、邀请统计与脱敏返利记录。 */
export function InvitationPage() {
  const { t, i18n } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const invitationsQuery = useUserInvitations()
  const [copied, setCopied] = useState<CopyTarget>()
  const [copyFailed, setCopyFailed] = useState(false)
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const formatNumber = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const formatTime = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])
  const invitations = invitationsQuery.data
  const inviteLink = invitations ? buildInviteLink(invitations.invite_code) : ''

  useEffect(() => {
    if (!copied) return undefined
    const timer = window.setTimeout(() => setCopied(undefined), 2_000)
    return () => window.clearTimeout(timer)
  }, [copied])

  const copyValue = async (target: CopyTarget, value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopyFailed(false)
      setCopied(target)
    } catch {
      setCopyFailed(true)
      setCopied(undefined)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <TicketCheck className="size-3.5" aria-hidden="true" />
            {t('invitations.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('invitations.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('invitations.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={invitationsQuery.isFetching}
          onClick={() => void invitationsQuery.refetch()}
        >
          <RefreshCw className={invitationsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('invitations.actions.refresh')}
        </Button>
      </header>

      {invitationsQuery.isPending ? (
        <InvitationLoading />
      ) : invitationsQuery.isError || !invitations ? (
        <InvitationError onRetry={() => void invitationsQuery.refetch()} />
      ) : (
        <>
          <InvitationSharePanel
            invitations={invitations}
            inviteLink={inviteLink}
            copied={copied}
            copyFailed={copyFailed}
            onCopy={copyValue}
          />
          <InvitationMetrics invitations={invitations} formatNumber={formatNumber.format} formatQuota={formatQuota} />
          <RecentRebates
            invitations={invitations}
            formatQuota={formatQuota}
            formatTime={(value) => formatTime.format(value * 1_000)}
          />
        </>
      )}
    </div>
  )
}

function InvitationSharePanel({
  invitations,
  inviteLink,
  copied,
  copyFailed,
  onCopy,
}: {
  invitations: UserInvitationSummaryResponse
  inviteLink: string
  copied?: CopyTarget
  copyFailed: boolean
  onCopy: (target: CopyTarget, value: string) => Promise<void>
}) {
  const { t } = useTranslation()
  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <CardTitle className="text-base">{t('invitations.share.title')}</CardTitle>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('invitations.share.subtitle')}</p>
          </div>
          <Badge className="border-success/25 bg-success/10 text-success">
            <ShieldCheck className="size-3" aria-hidden="true" />
            {t('invitations.share.private')}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="grid gap-4 p-5 lg:grid-cols-2">
        <CopyField
          id="invitation-code"
          label={t('invitations.share.code')}
          value={invitations.invite_code}
          copied={copied === 'code'}
          onCopy={() => onCopy('code', invitations.invite_code)}
        />
        <CopyField
          id="invitation-link"
          label={t('invitations.share.link')}
          value={inviteLink}
          copied={copied === 'link'}
          onCopy={() => onCopy('link', inviteLink)}
        />
        {copyFailed ? (
          <p role="alert" className="text-xs text-destructive lg:col-span-2">
            {t('invitations.errors.copy')}
          </p>
        ) : null}
      </CardContent>
    </Card>
  )
}

function CopyField({
  id,
  label,
  value,
  copied,
  onCopy,
}: {
  id: string
  label: string
  value: string
  copied: boolean
  onCopy: () => Promise<void>
}) {
  const { t } = useTranslation()
  const actionLabel = t(copied ? 'invitations.actions.copied' : 'invitations.actions.copy', { label })
  return (
    <div className="grid gap-1.5">
      <label htmlFor={id} className="text-xs font-medium">{label}</label>
      <div className="flex gap-2">
        <Input id={id} value={value} readOnly className="min-w-0 font-mono text-xs" />
        <Button
          type="button"
          size="icon"
          variant="outline"
          className="shrink-0"
          aria-label={actionLabel}
          title={actionLabel}
          onClick={() => void onCopy()}
        >
          {copied
            ? <Check className="size-4 text-success" aria-hidden="true" />
            : <Copy className="size-4" aria-hidden="true" />}
        </Button>
      </div>
    </div>
  )
}

function InvitationMetrics({
  invitations,
  formatNumber,
  formatQuota,
}: {
  invitations: UserInvitationSummaryResponse
  formatNumber: (value: number) => string
  formatQuota: (value: number) => string
}) {
  const { t } = useTranslation()
  const metrics = [
    { key: 'invited', icon: UsersRound, value: formatNumber(invitations.invited_count) },
    { key: 'credited', icon: Check, value: formatNumber(invitations.credited_count) },
    { key: 'current', icon: Coins, value: formatQuota(invitations.current_rebate_quota) },
    { key: 'history', icon: History, value: formatQuota(invitations.historical_rebate_quota) },
  ] as const
  return (
    <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label={t('invitations.metrics.label')}>
      {metrics.map((metric) => {
        const Icon = metric.icon
        return (
          <Card key={metric.key}>
            <CardContent className="p-4">
              <div className="flex items-center justify-between gap-3">
                <span className="text-xs text-muted-foreground">{t(`invitations.metrics.${metric.key}`)}</span>
                <Icon className="size-4 text-brand" aria-hidden="true" />
              </div>
              <p className="mt-3 text-xl font-semibold tabular-nums">{metric.value}</p>
            </CardContent>
          </Card>
        )
      })}
    </section>
  )
}

function RecentRebates({
  invitations,
  formatQuota,
  formatTime,
}: {
  invitations: UserInvitationSummaryResponse
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex items-center justify-between gap-3">
          <CardTitle className="text-base">{t('invitations.recent.title')}</CardTitle>
          <Badge className="bg-surface-2 text-muted-foreground">
            {t('invitations.recent.count', { count: invitations.recent_rebates.length })}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="p-0">
        {invitations.recent_rebates.length === 0 ? (
          <div className="grid min-h-36 place-items-center p-6 text-center">
            <div>
              <Link2 className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
              <p className="mt-2 text-sm font-medium">{t('invitations.recent.emptyTitle')}</p>
              <p className="mt-1 text-xs text-muted-foreground">{t('invitations.recent.emptyBody')}</p>
            </div>
          </div>
        ) : (
          <div className="divide-y divide-[var(--hairline)]">
            {invitations.recent_rebates.map((rebate, index) => (
              <div key={`${rebate.credited_at}-${index}`} className="flex items-center justify-between gap-4 px-5 py-4">
                <div className="min-w-0">
                  <p className="text-sm font-medium">{t('invitations.recent.rebate')}</p>
                  <p className="mt-1 text-xs text-muted-foreground">{formatTime(rebate.credited_at)}</p>
                </div>
                <Badge className="shrink-0 border-success/25 bg-success/10 text-success tabular-nums">
                  +{formatQuota(rebate.quota_amount)}
                </Badge>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function InvitationLoading() {
  return (
    <div className="grid gap-4" role="status">
      <Skeleton className="h-48 rounded-xl" />
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-28 rounded-xl" />)}
      </div>
      <Skeleton className="h-56 rounded-xl" />
    </div>
  )
}

function InvitationError({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-5">
      <h3 className="text-sm font-semibold text-destructive">{t('invitations.errors.loadTitle')}</h3>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('invitations.errors.loadBody')}</p>
      <Button type="button" size="sm" variant="secondary" className="mt-4" onClick={onRetry}>
        {t('invitations.actions.retry')}
      </Button>
    </div>
  )
}

function buildInviteLink(inviteCode: string) {
  return `${window.location.origin}${window.location.pathname}#/register?invite=${encodeURIComponent(inviteCode)}`
}
