import { Brain, Coins, Gauge, Route, UserRound, Zap, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { cn } from '@/lib/utils'
import { formatUsageDateLong, formatUsageLatency, formatUsageNumber, usageLatencyTone, type UsageLatencyKind } from './usage-log-format'
import { isAdminUsageLog, type UsageLogRow, usageLogCacheTokens, usageLogTotalTokens } from './usage-log-model'

type UsageLogDetailsSheetProps = {
  log?: UsageLogRow
  admin: boolean
  showPrincipal?: boolean
  formatQuota: (value: number) => string
  onOpenChange: (open: boolean) => void
  onTraceOpen: (requestId: string) => void
  onUserSelect?: (userId: number) => void
}

type DetailItem = {
  label: string
  value: string
  mono?: boolean
  wide?: boolean
  action?: () => void
  valueClassName?: string
}

function DetailSection({ title, icon: Icon, items }: { title: string; icon: LucideIcon; items: DetailItem[] }) {
  return (
    <section className="border-b border-[var(--hairline)] py-5 last:border-b-0">
      <h3 className="flex items-center gap-2 text-xs font-semibold">
        <Icon className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {title}
      </h3>
      <dl className="mt-3 grid grid-cols-2 gap-x-6 gap-y-4">
        {items.map((item) => (
          <div key={item.label} className={cn('min-w-0', item.wide && 'col-span-2')}>
            <dt className="text-[0.6875rem] text-muted-foreground">{item.label}</dt>
            <dd className={cn('mt-1 break-words text-sm tabular-nums', item.mono && 'font-mono text-xs', item.valueClassName)} title={item.value}>
              {item.action ? <button type="button" className="text-left font-medium text-brand outline-none hover:underline focus-visible:underline" onClick={item.action}>{item.value}</button> : item.value}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

/** 详情抽屉集中展示核账事实，主表只保留高频比较字段。 */
export function UsageLogDetailsSheet({ log, admin, showPrincipal = admin, formatQuota, onOpenChange, onTraceOpen, onUserSelect }: UsageLogDetailsSheetProps) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const number = (value: number) => formatUsageNumber(value, locale)
  const latency = (value: number | null) => formatUsageLatency(value, locale)
  const latencyClass = (value: number | null, kind: UsageLatencyKind) => {
    const tone = usageLatencyTone(value, kind)
    return tone === 'success' ? 'text-success' : tone === 'warning' ? 'text-warning' : tone === 'destructive' ? 'text-destructive' : 'text-muted-foreground'
  }

  return (
    <Sheet open={log !== undefined} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="usage-log-detail-description">
        {log ? (
          <>
            <SheetHeader className="border-b border-[var(--hairline)] px-5 py-4 pr-12">
              <div className="flex flex-wrap items-center gap-2">
                <Badge className="border-transparent bg-success/10 text-success">
                  <span className="mr-1.5 size-1.5 rounded-full bg-success" aria-hidden="true" />
                  {t('usageLogs.status.settled')}
                </Badge>
                <Badge className="border-transparent bg-info/10 text-info">
                  {log.protocol ? t(`usageLogs.protocol.${log.protocol}`) : t('usageLogs.values.legacy')}
                </Badge>
                {log.operation ? <Badge>{t(`usageLogs.operation.${log.operation}`)}</Badge> : null}
              </div>
              <SheetTitle className="mt-2 break-words text-lg">{log.model ?? t('usageLogs.values.legacyModel')}</SheetTitle>
              <SheetDescription id="usage-log-detail-description">
                {formatUsageDateLong(log.created_at, locale)}
              </SheetDescription>
            </SheetHeader>

            <div className="min-h-0 flex-1 overflow-y-auto px-5">
              <DetailSection
                title={t('usageLogs.detailSections.request')}
                icon={Route}
                items={[
                  {
                    label: t('usageLogs.details.request'),
                    value: log.request_id ?? `#${log.id}`,
                    mono: true,
                    wide: true,
                    action: admin && log.request_id ? () => onTraceOpen(log.request_id as string) : undefined,
                  },
                  { label: t('usageLogs.detailFields.responseMode'), value: log.is_stream === null ? t('usageLogs.values.legacy') : t(log.is_stream ? 'usageLogs.values.stream' : 'usageLogs.values.sync') },
                  { label: t('usageLogs.detailFields.protocol'), value: log.protocol ? t(`usageLogs.protocol.${log.protocol}`) : t('usageLogs.values.legacy') },
                  { label: t('usageLogs.detailFields.operation'), value: log.operation ? t(`usageLogs.operation.${log.operation}`) : t('usageLogs.values.notSpecified') },
                ]}
              />

              <DetailSection
                title={t('usageLogs.detailSections.performance')}
                icon={Gauge}
                items={[
                  { label: t('usageLogs.values.firstToken'), value: latency(log.first_token_ms), valueClassName: cn('font-semibold', latencyClass(log.first_token_ms, 'first-token')) },
                  { label: t('usageLogs.values.duration'), value: latency(log.duration_ms), valueClassName: cn('font-semibold', latencyClass(log.duration_ms, 'duration')) },
                ]}
              />

              <DetailSection
                title={t('usageLogs.detailSections.tokens')}
                icon={Zap}
                items={[
                  { label: t('usageLogs.detailFields.totalTokens'), value: number(usageLogTotalTokens(log)) },
                  { label: t('usageLogs.values.inputLabel'), value: number(log.input_tokens) },
                  { label: t('usageLogs.values.outputLabel'), value: number(log.output_tokens) },
                  { label: t('usageLogs.detailFields.cacheRead'), value: number(log.cache_read) },
                  { label: t('usageLogs.detailFields.cacheCreate5m'), value: number(log.cache_creation_5m) },
                  { label: t('usageLogs.detailFields.cacheCreate1h'), value: number(log.cache_creation_1h) },
                  { label: t('usageLogs.detailFields.cacheTotal'), value: number(usageLogCacheTokens(log)) },
                  { label: t('usageLogs.detailFields.reasoningTokens'), value: number(log.reasoning_tokens) },
                ]}
              />

              <DetailSection
                title={t('usageLogs.detailSections.billing')}
                icon={Coins}
                items={[
                  { label: t('usageLogs.values.quota'), value: formatQuota(log.quota) },
                  { label: t('usageLogs.detailFields.billingMode'), value: t(`usageLogs.values.billingMode.${log.billing_mode}`) },
                  { label: t('usageLogs.detailFields.usageSource'), value: 'usage_source' in log ? t(`usageLogs.values.source.${log.usage_source}`) : t('usageLogs.values.notRecorded') },
                  { label: t('usageLogs.detailFields.usageSemantics'), value: 'usage_semantics' in log ? t(`usageLogs.values.semantics.${log.usage_semantics}`) : t('usageLogs.values.notRecorded') },
                ]}
              />

              <DetailSection
                title={t('usageLogs.detailSections.reasoning')}
                icon={Brain}
                items={[
                  { label: t('usageLogs.detailFields.reasoningEffort'), value: log.reasoning_effort ? t(`usageLogs.reasoning.${log.reasoning_effort}`) : t('usageLogs.values.notSpecified') },
                  { label: t('usageLogs.detailFields.reasoningBudget'), value: log.reasoning_budget_tokens === null ? t('usageLogs.values.notSpecified') : number(log.reasoning_budget_tokens) },
                ]}
              />

              {showPrincipal && isAdminUsageLog(log) ? (
                <DetailSection
                  title={t('usageLogs.detailSections.principal')}
                  icon={UserRound}
                  items={[
                    { label: t('usageLogs.detailFields.user'), value: `${log.username} (#${log.user_id})`, action: onUserSelect ? () => onUserSelect(log.user_id) : undefined },
                    { label: t('usageLogs.detailFields.token'), value: `#${log.token_id}` },
                    { label: t('usageLogs.detailFields.group'), value: `#${log.group_id}` },
                  ]}
                />
              ) : null}
            </div>

          </>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
