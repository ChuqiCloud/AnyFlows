import { useTranslation } from 'react-i18next'

import type { UsageLogSummary as UsageLogSummaryValue } from './usage-log-model'

type UsageLogSummaryProps = {
  summary: UsageLogSummaryValue
  formatQuota: (value: number) => string
  /** 启用时间范围时把范围本身也列出来，避免只看数字不知道口径。 */
  range?: { label: string; value: string }
}

const toneClasses: Record<string, string> = {
  quota: 'bg-sky-500/70',
  rpm: 'bg-rose-500/65',
  tpm: 'bg-slate-400/70',
} as const

/** 单行指标带呈现当前页事实，避免管理台出现重复卡片墙。 */
export function UsageLogSummary({ summary, formatQuota, range }: UsageLogSummaryProps) {
  const { t, i18n } = useTranslation()
  const number = (value: number) => new Intl.NumberFormat(i18n.language).format(value)
  const items = [
    { key: 'quota', label: t('usageLogs.summary.usage'), value: formatQuota(summary.quota) },
    { key: 'rpm', label: t('usageLogs.summary.rpm'), value: number(summary.rpm) },
    { key: 'tpm', label: t('usageLogs.summary.tpm'), value: number(summary.tpm) },
    ...(range ? [{ key: 'range' as const, label: range.label, value: range.value }] : []),
  ]

  return (
    <section className="flex flex-wrap items-center gap-2" aria-label={t('usageLogs.summary.label')}>
      {items.map(({ key, label, value }) => (
        <span key={key} className="inline-flex h-7 items-center gap-2 rounded-md border border-[var(--hairline)] bg-surface-2/40 px-2.5 text-xs">
          <span className={`h-3.5 w-0.5 rounded-full ${key === 'range' ? 'bg-muted-foreground/50' : toneClasses[key]}`} aria-hidden="true" />
          <span className="text-muted-foreground">{label}</span>
          <span className="font-mono font-semibold tabular-nums text-foreground/85">{value}</span>
        </span>
      ))}
    </section>
  )
}
