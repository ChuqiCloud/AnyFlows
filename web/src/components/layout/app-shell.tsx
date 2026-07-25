import { Activity } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AppNavigation } from '@/components/layout/app-navigation'
import { LanguageToggle } from '@/components/language-toggle'
import { ThemeToggle } from '@/components/theme-toggle'

type AppShellProps = {
  title: string
}

const summaryItems = ['requests', 'quota', 'channels', 'errors']
const queueItems = ['auth', 'routing', 'billing']

export function AppShell({ title }: AppShellProps) {
  const { t } = useTranslation()

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <div className="grid min-h-dvh lg:grid-cols-[15rem_minmax(0,1fr)]">
        <aside className="hidden border-r bg-sidebar text-sidebar-foreground lg:flex lg:flex-col">
          <div className="flex h-16 items-center gap-3 border-b px-5">
            <BrandMark />
            <div className="leading-tight">
              <div className="font-semibold">{t('brand.name')}</div>
              <div className="text-xs text-muted-foreground">{t('brand.scope')}</div>
            </div>
          </div>
          <AppNavigation className="flex-1 flex-col px-3 py-4" />
        </aside>

        <div className="flex min-w-0 flex-col">
          <header className="flex h-16 items-center justify-between border-b bg-background/95 px-4 backdrop-blur md:px-6">
            <div className="flex min-w-0 items-center gap-3">
              <div className="lg:hidden">
                <BrandMark />
              </div>
              <div className="min-w-0">
                <p className="text-xs font-medium text-muted-foreground">{t('brand.name')}</p>
                <h1 className="truncate text-lg font-semibold">{title}</h1>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <LanguageToggle />
              <ThemeToggle />
            </div>
          </header>
          <AppNavigation compact className="border-b px-4 py-2 lg:hidden" />

          <main className="flex-1 px-4 py-6 md:px-6 lg:px-8">
            <div className="mx-auto flex w-full max-w-[1400px] flex-col gap-6">
              <section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4" aria-label={t('summary.label')}>
                {summaryItems.map((item) => (
                  <article key={item} className="rounded-lg border bg-card p-4 text-card-foreground shadow-sm">
                    <div className="text-sm text-muted-foreground">{t(`summary.${item}.label`)}</div>
                    <div className="mt-3 text-2xl font-semibold tabular-nums">{t(`summary.${item}.value`)}</div>
                    <div className="mt-2 text-xs text-muted-foreground">{t(`summary.${item}.hint`)}</div>
                  </article>
                ))}
              </section>

              <section className="grid gap-6 xl:grid-cols-[minmax(0,1.55fr)_minmax(18rem,0.75fr)]">
                <article className="rounded-lg border bg-card p-5 text-card-foreground shadow-sm">
                  <div className="border-b pb-4">
                    <h2 className="text-base font-semibold">{t('workspace.title')}</h2>
                    <p className="mt-1 text-sm text-muted-foreground">{t('workspace.subtitle')}</p>
                  </div>
                  <div className="grid min-h-72 place-items-center py-8">
                    <div className="max-w-sm text-center">
                      <div className="mx-auto grid size-12 place-items-center rounded-lg border bg-muted text-muted-foreground">
                        <Activity className="size-5" aria-hidden="true" />
                      </div>
                      <h3 className="mt-4 text-sm font-semibold">{t('empty.title')}</h3>
                      <p className="mt-2 text-sm text-muted-foreground">{t('empty.body')}</p>
                    </div>
                  </div>
                </article>

                <article className="rounded-lg border bg-card p-5 text-card-foreground shadow-sm">
                  <h2 className="text-base font-semibold">{t('queue.title')}</h2>
                  <div className="mt-4 divide-y">
                    {queueItems.map((item) => (
                      <div key={item} className="grid grid-cols-[1fr_auto] gap-4 py-3 text-sm">
                        <span>{t(`queue.${item}.label`)}</span>
                        <span className="text-muted-foreground">{t(`queue.${item}.state`)}</span>
                      </div>
                    ))}
                  </div>
                </article>
              </section>
            </div>
          </main>
        </div>
      </div>
    </div>
  )
}

function BrandMark() {
  return (
    <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-primary text-sm font-semibold text-primary-foreground shadow-sm">
      AF
    </div>
  )
}
