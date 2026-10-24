import { Badge } from '@/components/ui/badge'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { CircleAlert, Clock3, Route, UserRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DetailSection } from './usage-log-detail-section'
import { formatUsageDateLong, formatUsageLatency } from './usage-log-format'
import { isAdminFailedCallLog, type FailedCallLogRow } from './usage-log-model'

type FailedCallLogDetailsSheetProps = {
  log?: FailedCallLogRow
  admin: boolean
  showPrincipal?: boolean
  onOpenChange: (open: boolean) => void
  onTraceOpen: (requestId: string) => void
  onUserSelect?: (userId: number) => void
}

function FailedCallLogDetails({ log, admin, showPrincipal, onTraceOpen, onUserSelect }: Omit<FailedCallLogDetailsSheetProps, 'log' | 'onOpenChange'> & { log: FailedCallLogRow }) {
  const { t, i18n } = useTranslation()
  const internal = admin && showPrincipal && isAdminFailedCallLog(log) ? log : undefined
  const id = (value: number | null) => value == null ? t('usageLogs.values.notRecorded') : `#${value}`
  const userId = internal?.user_id

  return (
    <>
      <DetailSection
        title={t('usageLogs.detailSections.request')}
        icon={Route}
        items={[
          { label: t('usageLogs.details.request'), value: log.request_id || `#${log.id}`, mono: true, wide: true, copyable: Boolean(log.request_id), action: admin && log.request_id ? () => onTraceOpen(log.request_id) : undefined },
          { label: t('usageLogs.detailFields.protocol'), value: t(`usageLogs.protocol.${log.protocol}`) },
          { label: t('usageLogs.detailFields.operation'), value: t(`usageLogs.operation.${log.operation}`) },
        ]}
      />
      <DetailSection
        title={t('usageLogs.failed.details')}
        icon={CircleAlert}
        items={[
          { label: t('usageLogs.failed.errorCode'), value: log.error_code, mono: true, wide: true, copyable: true, valueClassName: 'text-destructive' },
          { label: t('usageLogs.failed.errorMessage'), value: log.error_message, wide: true, copyable: true },
          ...(internal ? [{ label: t('usageLogs.failed.errorKind'), value: internal.error_kind, mono: true, wide: true }] : []),
        ]}
      />
      <DetailSection
        title={t('usageLogs.detailSections.performance')}
        icon={Clock3}
        items={[{ label: t('usageLogs.values.duration'), value: formatUsageLatency(log.duration_ms, i18n.language) }]}
      />
      {internal ? (
        <DetailSection
          title={t('usageLogs.failed.principal')}
          icon={UserRound}
          items={[
            { label: t('usageLogs.detailFields.user'), value: userId == null ? internal.username || t('usageLogs.failed.unknownUser') : internal.username ? `${internal.username} (#${userId})` : `#${userId}`, action: userId != null && onUserSelect ? () => onUserSelect(userId) : undefined },
            { label: t('usageLogs.detailFields.token'), value: id(internal.token_id) },
            { label: t('usageLogs.detailFields.group'), value: id(internal.group_id) },
            { label: t('usageLogs.failed.columns.channel'), value: id(internal.channel_id) },
            { label: t('usageLogs.failed.organization'), value: id(internal.organization_id) },
            { label: t('usageLogs.failed.team'), value: id(internal.organization_team_id) },
          ]}
        />
      ) : null}
    </>
  )
}

export function FailedCallLogDetailsSheet({ log, admin, showPrincipal = admin, onOpenChange, onTraceOpen, onUserSelect }: FailedCallLogDetailsSheetProps) {
  const { t, i18n } = useTranslation()

  return (
    <Sheet open={log !== undefined} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="failed-call-log-detail-description">
        {log ? (
          <>
            <SheetHeader className="border-b border-[var(--hairline)] px-5 py-4 pr-12">
              <div className="flex flex-wrap items-center gap-2">
                <Badge className="border-transparent bg-destructive/10 text-destructive"><CircleAlert className="mr-1 size-3" aria-hidden="true" />{t('usageLogs.failed.status')}</Badge>
                <Badge className="border-transparent bg-info/10 text-info">{t(`usageLogs.protocol.${log.protocol}`)}</Badge>
                <Badge>{t(`usageLogs.operation.${log.operation}`)}</Badge>
              </div>
              <SheetTitle className="mt-2 [overflow-wrap:anywhere] text-lg">{log.model}</SheetTitle>
              <SheetDescription id="failed-call-log-detail-description">{t('usageLogs.failed.title')} · {formatUsageDateLong(log.created_at, i18n.language)}</SheetDescription>
            </SheetHeader>
            <div className="min-h-0 flex-1 overflow-y-auto px-5">
              <FailedCallLogDetails log={log} admin={admin} showPrincipal={showPrincipal} onTraceOpen={onTraceOpen} onUserSelect={onUserSelect} />
            </div>
          </>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
