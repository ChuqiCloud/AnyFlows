import { Button } from '@heroui/react'
import { Boxes, Braces } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { LanguageToggle } from '@/components/language-toggle'
import { MotionToggle } from '@/components/motion-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
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
        <RouterLink
          to="/"
          className="flex min-w-0 items-center gap-2.5 rounded-lg focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
        >
          <SiteBrandMark siteName={siteName} logoUrl={logoUrl} />
          <span className="hidden truncate text-[0.9375rem] font-semibold sm:inline">
            {siteName}
          </span>
        </RouterLink>
        <div className="flex shrink-0 items-center gap-1.5">
          <Button as={RouterLink} to="/models" size="sm" title={t('landing.nav.models')} variant="light">
            <Boxes className="size-4" aria-hidden="true" />
            <span className="hidden md:inline">{t('landing.nav.models')}</span>
            <span className="sr-only md:hidden">{t('landing.nav.models')}</span>
          </Button>
          <Button as={RouterLink} to="/api" size="sm" title={t('apiExplorer.nav')} variant="light">
            <Braces className="size-4" aria-hidden="true" />
            <span className="hidden md:inline">API</span>
            <span className="sr-only md:hidden">API</span>
          </Button>
          <LanguageToggle />
          <MotionToggle />
          <ThemeToggle />
          <Button as={RouterLink} color="primary" to="/console" size="sm">{t('landing.nav.console')}</Button>
        </div>
      </div>
    </header>
  )
}
