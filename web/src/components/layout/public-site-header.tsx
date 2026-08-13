import { Boxes } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LanguageToggle } from '@/components/language-toggle'
import { MotionToggle } from '@/components/motion-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import { Button } from '@/components/ui/button'
import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'

type PublicSiteHeaderProps = {
  logoUrl?: string | null
  siteName: string
}

/** 公开页面共用同一站点身份与导航，避免跨页面时顶栏结构和操作入口跳变。 */
export function PublicSiteHeader({ logoUrl, siteName }: PublicSiteHeaderProps) {
  const { t } = useTranslation()

  return (
    <header className="fixed inset-x-0 top-0 z-50 border-b border-[var(--hairline)] bg-background/80 backdrop-blur-xl">
      <div className="mx-auto flex h-14 max-w-[1120px] items-center justify-between px-3 sm:px-6">
        <a
          href="#/"
          className="flex min-w-0 items-center gap-2.5 rounded-lg focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
        >
          <SiteBrandMark siteName={siteName} logoUrl={logoUrl} />
          <span className="hidden truncate text-[0.9375rem] font-semibold sm:inline">
            {siteName}
          </span>
        </a>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button variant="ghost" size="sm" asChild>
            <a href="#/models" title={t('landing.nav.models')}>
              <Boxes className="size-4" aria-hidden="true" />
              <span className="hidden md:inline">{t('landing.nav.models')}</span>
              <span className="sr-only md:hidden">{t('landing.nav.models')}</span>
            </a>
          </Button>
          <LanguageToggle />
          <MotionToggle />
          <ThemeToggle />
          <Button size="sm" asChild>
            <a href="#/console">{t('landing.nav.console')}</a>
          </Button>
        </div>
      </div>
    </header>
  )
}
