import { CheckCircle2, FileText, Layers, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export function TransparencyBento() {
  const { t } = useTranslation()
  const upstreams = [
    { name: t('landing.transparency.probesName'), detail: t('landing.transparency.probesDetail') },
    { name: t('landing.transparency.credentialsName'), detail: t('landing.transparency.credentialsDetail') },
    { name: t('landing.transparency.retryName'), detail: t('landing.transparency.retryDetail') },
    { name: t('landing.transparency.tracesName'), detail: t('landing.transparency.tracesDetail') },
  ]

  const recentLogs = [
    { field: 'request_id', description: t('landing.transparency.requestIdDescription') },
    { field: 'model', description: t('landing.transparency.modelDescription') },
    { field: 'usage', description: t('landing.transparency.usageDescription') },
    { field: 'billed_cost', description: t('landing.transparency.costDescription') },
  ]

  return (
    <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      {/* Header */}
      <div className="max-w-2xl">
        <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
          <ShieldCheck className="size-3.5" />
          <span>{t('landing.transparency.eyebrow')}</span>
        </div>
        <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
          {t('landing.transparency.heading')}
        </h2>
        <p className="mt-2.5 text-base leading-7 text-muted-foreground">
          {t('landing.transparency.description')}
        </p>
      </div>

      {/* 3 Bento Cards */}
      <div className="mt-12 grid gap-6 lg:grid-cols-3">
        {/* Card 1: Official Service Status */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t('landing.transparency.statusEyebrow')}
              </span>
              <span className="inline-flex items-center gap-1.5 rounded-full bg-success/15 px-2.5 py-0.5 text-xs font-semibold text-success">
                <span className="size-1.5 rounded-full bg-success animate-pulse" />
                {t('landing.transparency.statusBadge')}
              </span>
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              {t('landing.transparency.statusTitle')}
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('landing.transparency.statusDescription')}
            </p>

            <div className="mt-6 space-y-2.5">
              {upstreams.map((item) => (
                <div
                  key={item.name}
                  className="flex items-center justify-between rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 text-xs"
                >
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-success shrink-0" />
                    <span className="font-medium text-foreground">{item.name}</span>
                  </div>
                  <div className="flex items-center gap-2.5 font-mono text-[0.6875rem]">
                    <span className="text-muted-foreground">{item.detail}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>{t('landing.transparency.statusFootnote')}</span>
            <span className="font-mono text-[0.6875rem]">{t('landing.transparency.statusPolicy')}</span>
          </div>
        </div>

        {/* Card 2: Granular Request Traces & Billing */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t('landing.transparency.billingEyebrow')}
              </span>
              <FileText className="size-4 text-muted-foreground" />
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              {t('landing.transparency.billingTitle')}
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('landing.transparency.billingDescription')}
            </p>

            <div className="mt-6 space-y-2 font-mono text-[0.6875rem]">
              {recentLogs.map((log) => (
                <div
                  key={log.field}
                  className="rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 flex items-center justify-between"
                >
                  <div>
                    <div className="text-foreground font-medium">{log.field}</div>
                    <div className="text-muted-foreground text-[0.625rem]">
                      {log.description}
                    </div>
                  </div>
                  <div className="text-right">
                    <span className="text-muted-foreground">{t('landing.transparency.fieldDescription')}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>{t('landing.transparency.billingAccess')}</span>
            <span className="font-mono text-[0.6875rem]">{t('landing.transparency.billingRecord')}</span>
          </div>
        </div>

        {/* Card 3: IR Protocol Engine */}
        <div className="rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t('landing.transparency.protocolEyebrow')}
              </span>
              <Layers className="size-4 text-muted-foreground" />
            </div>
            <h3 className="mt-4 text-lg font-semibold text-foreground">
              {t('landing.transparency.protocolTitle')}
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('landing.transparency.protocolDescription')}
            </p>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-4 space-y-3 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground font-mono text-[0.6875rem]">{t('landing.transparency.client')}</span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-foreground font-mono text-[0.6875rem]">
                  {t('landing.transparency.clientExamples')}
                </span>
              </div>
              <div className="flex items-center justify-center">
                <div className="h-4 w-px bg-primary/40" />
              </div>
              <div className="rounded-lg border border-primary/30 bg-primary/10 p-2 text-center font-mono text-xs font-semibold text-primary">
                {t('landing.transparency.canonical')}
              </div>
              <div className="flex items-center justify-center">
                <div className="h-4 w-px bg-primary/40" />
              </div>
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground font-mono text-[0.6875rem]">{t('landing.transparency.upstream')}</span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-foreground font-mono text-[0.6875rem]">
                  {t('landing.transparency.providerTargets')}
                </span>
              </div>
            </div>
          </div>

          <div className="mt-6 pt-4 border-t border-[var(--hairline)] flex items-center justify-between text-xs text-muted-foreground">
            <span>{t('landing.transparency.protocolReasoning')}</span>
            <span className="font-mono text-[0.6875rem]">{t('landing.transparency.protocolStreaming')}</span>
          </div>
        </div>
      </div>
    </section>
  )
}
