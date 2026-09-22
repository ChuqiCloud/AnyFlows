import { CircleAlert, Clock3, Server, UserRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Chip } from '@heroui/react'
import { formatCompactRequestId, formatUsageDate, formatUsageLatency } from './usage-log-format'
import { isAdminFailedCallLog, type FailedCallLogRow } from './usage-log-model'

type FailedCallTableProps = {
  logs: FailedCallLogRow[]
  showInternal?: boolean
  filtered: boolean
}

function protocolLabel(protocol: FailedCallLogRow['protocol'], t: ReturnType<typeof useTranslation>['t']) {
  return t(`usageLogs.protocol.${protocol}`)
}

/** 失败调用单独成表，普通用户只会收到服务端裁剪后的安全字段。 */
export function FailedCallTable({ logs, showInternal = false, filtered }: FailedCallTableProps) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language

  if (logs.length === 0) {
    return (
      <div className="grid min-h-48 place-items-center rounded-lg border border-[var(--hairline)] py-8 text-center">
        <div className="max-w-sm px-5">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><CircleAlert className="size-4" aria-hidden="true" /></div>
          <h3 className="mt-3 text-sm font-semibold">{t(filtered ? 'usageLogs.failed.emptyFilteredTitle' : 'usageLogs.failed.emptyTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'usageLogs.failed.emptyFilteredBody' : 'usageLogs.failed.emptyBody')}</p>
        </div>
      </div>
    )
  }

  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('usageLogs.failed.listLabel')}>
      <div className="hidden grid-cols-[7rem_minmax(11rem,1fr)_minmax(14rem,1.3fr)_7rem_minmax(11rem,1fr)] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('usageLogs.failed.columns.time')}</span>
        <span>{t('usageLogs.failed.columns.call')}</span>
        <span>{t('usageLogs.failed.columns.error')}</span>
        <span>{t('usageLogs.failed.columns.duration')}</span>
        {showInternal ? <span>{t('usageLogs.failed.columns.internal')}</span> : <span />}
      </div>
      {logs.map((log) => {
        const internal = isAdminFailedCallLog(log)
        return (
          <article key={log.id} className="grid gap-3 border-b border-[var(--hairline)] px-3 py-3 last:border-b-0 lg:grid-cols-[7rem_minmax(11rem,1fr)_minmax(14rem,1.3fr)_7rem_minmax(11rem,1fr)] lg:items-start lg:px-4 lg:py-3">
            <div className="text-xs text-muted-foreground">
              <div className="tabular-nums text-foreground">{formatUsageDate(log.created_at, locale)}</div>
              <div className="mt-1 inline-flex items-center gap-1 text-[0.6875rem] text-destructive"><CircleAlert className="size-3" aria-hidden="true" />{t('usageLogs.failed.status')}</div>
            </div>

            <div className="min-w-0">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <span className="truncate text-sm font-semibold">{log.model}</span>
                <Chip size="sm" variant="flat" className="shrink-0 border-transparent bg-info/10 text-info">{protocolLabel(log.protocol, t)}</Chip>
              </div>
              <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground">
                <span>{t(`usageLogs.operation.${log.operation}`)}</span>
                <code className="truncate font-mono" title={log.request_id}>{formatCompactRequestId(log.request_id)}</code>
              </div>
            </div>

            <div className="min-w-0 rounded-md bg-destructive/5 px-2.5 py-2">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <Chip size="sm" variant="flat" className="border-transparent bg-destructive/10 font-mono text-destructive">{log.error_code}</Chip>
                {internal && showInternal ? <span className="truncate font-mono text-[0.6875rem] text-muted-foreground">{log.error_kind}</span> : null}
              </div>
              <p className="mt-1 break-words text-xs leading-5 text-foreground">{log.error_message}</p>
            </div>

            <div className="flex items-center gap-1 text-xs font-medium tabular-nums text-muted-foreground lg:block">
              <Clock3 className="mr-1 inline size-3" aria-hidden="true" />
              {formatUsageLatency(log.duration_ms, locale)}
            </div>

            {showInternal ? (
              <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground">
                {internal && log.username ? <span className="inline-flex items-center gap-1 font-medium text-foreground"><UserRound className="size-3" aria-hidden="true" />{log.username} (#{log.user_id ?? '-'})</span> : null}
                {internal && log.token_id ? <span>key #{log.token_id}</span> : null}
                {internal && log.group_id ? <span>group #{log.group_id}</span> : null}
                {internal && log.organization_id ? <span>org #{log.organization_id}</span> : null}
                {internal && log.organization_team_id ? <span>team #{log.organization_team_id}</span> : null}
                {internal && log.channel_id ? <span className="inline-flex items-center gap-1"><Server className="size-3" aria-hidden="true" />channel #{log.channel_id}</span> : null}
              </div>
            ) : <div />}

            <div className="grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-2 text-[0.6875rem] lg:hidden">
              <div>
                <span className="text-muted-foreground">{t('usageLogs.failed.columns.duration')}</span>
                <p className="mt-1 font-semibold tabular-nums">{formatUsageLatency(log.duration_ms, locale)}</p>
              </div>
              <div className="min-w-0">
                <span className="text-muted-foreground">{t('usageLogs.failed.columns.request')}</span>
                <p className="mt-1 truncate font-mono" title={log.request_id}>{log.request_id}</p>
              </div>
              {showInternal && internal ? <div className="col-span-2 text-muted-foreground">{t('usageLogs.failed.columns.internal')}: {log.error_kind} · {t('usageLogs.failed.columns.channel')} #{log.channel_id ?? '-'}</div> : null}
            </div>
          </article>
        )
      })}
    </section>
  )
}
