import { Button, Chip, Skeleton } from '@heroui/react'
import { MessageSquareText, RefreshCw } from 'lucide-react'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { BrandMark } from '@/components/layout/brand-mark'
import { LanguageToggle } from '@/components/language-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import { usePublicPlaygroundShare } from './playground-share-api'
import { classifyPublicShareError } from './playground-share-errors'
import { PlaygroundSharedResult } from './playground-shared-result'

type PublicPlaygroundSharePageProps = {
  token?: string
}

function PublicShareLoading() {
  const { t } = useTranslation()

  return (
    <div className="grid gap-3" role="status">
      <span className="sr-only">{t('playground.share.public.loading')}</span>
      <Skeleton className="h-20 rounded-xl" aria-hidden="true" />
      <Skeleton className="h-[32rem] rounded-xl" aria-hidden="true" />
    </div>
  )
}

function PublicShareError({ unavailable, onRetry }: { unavailable: boolean; onRetry: () => void }) {
  const { t } = useTranslation()

  return (
    <section className="grid min-h-[24rem] place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 px-6 py-12 text-center">
      <div className="max-w-sm">
        <div className="mx-auto grid size-10 place-items-center rounded-xl bg-surface-2 text-muted-foreground">
          <MessageSquareText className="size-4" aria-hidden="true" />
        </div>
        <h1 className="mt-3 text-base font-semibold">
          {t(unavailable ? 'playground.share.public.unavailableTitle' : 'playground.share.public.notFoundTitle')}
        </h1>
        <p className="mt-1 text-sm leading-6 text-muted-foreground">
          {t(unavailable ? 'playground.share.public.unavailableBody' : 'playground.share.public.notFoundBody')}
        </p>
        {unavailable ? (
          <Button type="button" className="mt-4" variant="bordered" onClick={onRetry}>
            <RefreshCw className="size-4" aria-hidden="true" />{t('playground.share.public.retry')}
          </Button>
        ) : null}
      </div>
    </section>
  )
}

/** 游客只读分享页；不挂载安装状态、登录会话或控制台守卫。 */
export function PublicPlaygroundSharePage({ token }: PublicPlaygroundSharePageProps) {
  const { i18n, t } = useTranslation()
  const shareQuery = usePublicPlaygroundShare(token)

  useEffect(() => {
    const previousTitle = document.title
    const robots = document.createElement('meta')
    robots.name = 'robots'
    robots.content = 'noindex,nofollow'
    document.head.append(robots)
    document.title = t('playground.share.public.pageTitle')
    return () => {
      robots.remove()
      document.title = previousTitle
    }
  }, [t])

  const unavailable = shareQuery.isError
    && classifyPublicShareError(shareQuery.error) === 'unavailable'
  const data = shareQuery.data
  const formatTime = (value: number) => new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value * 1000)

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <header className="sticky top-0 z-30 border-b border-[var(--hairline)] bg-background/85 backdrop-blur-xl">
        <div className="mx-auto flex h-14 max-w-[1120px] items-center justify-between gap-3 px-3 sm:px-5">
          <a href="/" className="flex min-w-0 items-center gap-2.5 rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
            <BrandMark />
            <div className="hidden min-w-0 sm:block">
              <div className="truncate text-sm font-semibold">{t('brand.name')}</div>
              <div className="truncate text-[0.625rem] text-muted-foreground">{t('playground.share.public.headerScope')}</div>
            </div>
          </a>
          <div className="flex shrink-0 items-center gap-1.5">
            <LanguageToggle />
            <ThemeToggle />
            <Button as="a" href="/console/playground" size="sm" variant="bordered" className="px-2.5" aria-label={t('playground.share.public.openPlayground')}>
              <MessageSquareText className="size-3.5" aria-hidden="true" />
              <span className="hidden md:inline">{t('playground.share.public.openPlayground')}</span>
            </Button>
          </div>
        </div>
      </header>

      <main className="px-4 py-5 md:px-5 md:py-7">
        <div className="mx-auto grid w-full max-w-[1080px] gap-3">
          {!token ? (
            <PublicShareError unavailable={false} onRetry={() => undefined} />
          ) : shareQuery.isPending ? (
            <PublicShareLoading />
          ) : shareQuery.isError || !data ? (
            <PublicShareError unavailable={unavailable} onRetry={() => void shareQuery.refetch()} />
          ) : (
            <>
              <section className="flex flex-col gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <h1 className="text-lg font-semibold">{t('playground.share.public.title')}</h1>
                    <Chip className="bg-info/12 text-info" size="sm" variant="flat">{t('playground.share.public.readOnly')}</Chip>
                  </div>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('playground.share.public.description')}</p>
                </div>
                <dl className="grid shrink-0 grid-cols-2 gap-x-4 gap-y-1 text-xs tabular-nums sm:text-right">
                  <div><dt className="text-muted-foreground">{t('playground.share.public.created')}</dt><dd>{formatTime(data.created_at)}</dd></div>
                  <div><dt className="text-muted-foreground">{t('playground.share.public.expires')}</dt><dd>{formatTime(data.expires_at)}</dd></div>
                </dl>
              </section>
              <PlaygroundSharedResult sessions={data.sessions} />
            </>
          )}
        </div>
      </main>
    </div>
  )
}
