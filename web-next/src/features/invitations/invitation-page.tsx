import { Button, Card, CardBody, CardHeader, Chip, Input, Skeleton } from '@heroui/react'
import { Check, Copy, Link2, RefreshCw, ShieldCheck, TicketCheck } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
          variant="bordered"
          isDisabled={invitationsQuery.isFetching}
          onClick={() => void invitationsQuery.refetch()}
        >
          <RefreshCw className={invitationsQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          {t('invitations.actions.refresh')}
        </Button>
      </header>

      {invitationsQuery.isPending ? (
        <InvitationLoading />
      ) : invitationsQuery.isError || !invitations ? (
        <InvitationError onRetry={() => void invitationsQuery.refetch()} />
      ) : (
        <>
          <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(16rem,20rem)]">
            <InvitationSharePanel
              invitations={invitations}
              inviteLink={inviteLink}
              copied={copied}
              copyFailed={copyFailed}
              onCopy={copyValue}
            />
            <InvitationMetrics invitations={invitations} formatNumber={formatNumber.format} formatQuota={formatQuota} />
          </section>
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
  const linkAction = t(copied === 'link' ? 'invitations.actions.copied' : 'invitations.actions.copy', { label: t('invitations.share.link') })
  const codeAction = t(copied === 'code' ? 'invitations.actions.copied' : 'invitations.actions.copy', { label: t('invitations.share.code') })
  return (
    <Card className="border border-[var(--hairline)]" shadow="none">
      <CardBody className="flex flex-col gap-5 p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold">{t('invitations.share.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('invitations.share.subtitle')}</p>
          </div>
          <Chip className="border-success/25 bg-success/10 text-success" size="sm" startContent={<ShieldCheck className="size-3" aria-hidden="true" />} variant="flat">
            {t('invitations.share.private')}
          </Chip>
        </div>

        <div className="grid gap-1.5">
          <label htmlFor="invitation-link" className="text-xs font-medium">{t('invitations.share.link')}</label>
          <div className="flex gap-2">
            <Input classNames={{ input: 'font-mono text-xs' }} id="invitation-link" isReadOnly value={inviteLink} />
            <Button className="shrink-0" color="primary" type="button" onClick={() => void onCopy('link', inviteLink)}>
              {copied === 'link'
                ? <Check className="size-4" aria-hidden="true" />
                : <Copy className="size-4" aria-hidden="true" />}
              {linkAction}
            </Button>
          </div>
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('invitations.share.linkHint')}</p>
        </div>

        <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-t border-[var(--hairline)] pt-4 text-xs">
          <span className="text-muted-foreground">{t('invitations.share.code')}</span>
          <code className="rounded-md bg-surface-2 px-2 py-1 font-mono text-xs">{invitations.invite_code}</code>
          <Button
            isIconOnly
            aria-label={codeAction}
            size="sm"
            title={codeAction}
            type="button"
            variant="light"
            onClick={() => void onCopy('code', invitations.invite_code)}
          >
            {copied === 'code'
              ? <Check className="size-3.5 text-success" aria-hidden="true" />
              : <Copy className="size-3.5" aria-hidden="true" />}
          </Button>
          {copyFailed ? <span role="alert" className="text-destructive">{t('invitations.errors.copy')}</span> : null}
        </div>
      </CardBody>
    </Card>
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
  const rows = [
    { key: 'invited', value: formatNumber(invitations.invited_count) },
    { key: 'credited', value: formatNumber(invitations.credited_count) },
    { key: 'current', value: formatQuota(invitations.current_rebate_quota) },
  ] as const
  return (
    <Card className="border border-[var(--hairline)]" shadow="none">
      <CardBody className="flex flex-col p-5" aria-label={t('invitations.metrics.label')}>
        <span className="text-xs text-muted-foreground">{t('invitations.metrics.history')}</span>
        <p className="mt-2 truncate text-2xl font-semibold tabular-nums" title={formatQuota(invitations.historical_rebate_quota)}>
          {formatQuota(invitations.historical_rebate_quota)}
        </p>
        <dl className="mt-5 grid gap-2.5 border-t border-[var(--hairline)] pt-4 text-xs">
          {rows.map((row) => (
            <div key={row.key} className="flex items-center justify-between gap-3">
              <dt className="text-muted-foreground">{t(`invitations.metrics.${row.key}`)}</dt>
              <dd className="font-medium tabular-nums">{row.value}</dd>
            </div>
          ))}
        </dl>
      </CardBody>
    </Card>
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
    <Card className="overflow-hidden border border-[var(--hairline)]" shadow="none">
      <CardHeader className="flex-col items-stretch gap-1.5 border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex items-center justify-between gap-3">
          <h3 className="text-base font-semibold">{t('invitations.recent.title')}</h3>
          <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">
            {t('invitations.recent.count', { count: invitations.recent_rebates.length })}
          </Chip>
        </div>
      </CardHeader>
      <CardBody className="p-0">
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
                <Chip className="shrink-0 border-success/25 bg-success/10 text-success tabular-nums" size="sm" variant="flat">
                  +{formatQuota(rebate.quota_amount)}
                </Chip>
              </div>
            ))}
          </div>
        )}
      </CardBody>
    </Card>
  )
}

function InvitationLoading() {
  return (
    <div className="grid gap-4" role="status">
      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(16rem,20rem)]">
        <Skeleton className="h-52 rounded-xl" />
        <Skeleton className="h-52 rounded-xl" />
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
      <Button type="button" size="sm" variant="bordered" className="mt-4" onClick={onRetry}>
        {t('invitations.actions.retry')}
      </Button>
    </div>
  )
}

function buildInviteLink(inviteCode: string) {
  return `${window.location.origin}/register?invite=${encodeURIComponent(inviteCode)}`
}
