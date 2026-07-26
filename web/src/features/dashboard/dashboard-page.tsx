import { Activity } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

const summaryItems = ['requests', 'quota', 'channels', 'errors'] as const
const queueItems = ['auth', 'routing', 'billing'] as const

export function DashboardPage() {
  const { t } = useTranslation()

  return (
    <div className="flex flex-col gap-3">
      <section className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4" aria-label={t('summary.label')}>
        {summaryItems.map((item) => (
          <Card key={item} interactive className="p-3">
            <div className="text-xs text-muted-foreground">{t(`summary.${item}.label`)}</div>
            <div className="mt-2 text-xl font-semibold tabular-nums">{t(`summary.${item}.value`)}</div>
            <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t(`summary.${item}.hint`)}</div>
          </Card>
        ))}
      </section>

      <section className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_minmax(15rem,0.75fr)]">
        <Card>
          <CardHeader className="border-b border-[var(--hairline)] py-3 px-4">
            <CardTitle className="text-sm">{t('workspace.title')}</CardTitle>
            <CardDescription className="text-xs">{t('workspace.subtitle')}</CardDescription>
          </CardHeader>
          <CardContent className="grid min-h-52 place-items-center py-6">
            <div className="max-w-xs text-center">
              <div className="mx-auto grid size-9 place-items-center rounded-lg border border-[var(--hairline)] text-muted-foreground">
                <Activity className="size-4" aria-hidden="true" />
              </div>
              <h3 className="mt-3 text-xs font-semibold">{t('empty.title')}</h3>
              <p className="mt-1 text-xs text-muted-foreground">{t('empty.body')}</p>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="py-3 px-4">
            <CardTitle className="text-sm">{t('queue.title')}</CardTitle>
          </CardHeader>
          <CardContent className="px-4 pb-3 pt-0">
            <div className="divide-y divide-[var(--hairline)]">
              {queueItems.map((item) => (
                <div key={item} className="grid grid-cols-[1fr_auto] items-center gap-3 py-2 text-xs">
                  <span>{t(`queue.${item}.label`)}</span>
                  <span className="inline-flex items-center gap-1.5 text-muted-foreground">
                    <span className="size-1.5 rounded-full bg-muted-foreground/60" aria-hidden="true" />
                    {t(`queue.${item}.state`)}
                  </span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      </section>
    </div>
  )
}
