import { Button, Popover, PopoverContent, PopoverTrigger } from '@heroui/react'
import { Boxes, Braces, LayoutDashboard, SlidersHorizontal } from 'lucide-react'
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
      <div className="mx-auto flex h-14 max-w-[1120px] items-center justify-between gap-2 px-3 sm:px-6">
        <RouterLink
          to="/"
          className="flex min-w-0 shrink items-center gap-2.5 rounded-lg focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
        >
          <SiteBrandMark siteName={siteName} logoUrl={logoUrl} />
          <span className="hidden truncate text-[0.9375rem] font-semibold lg:inline">
            {siteName}
          </span>
        </RouterLink>
        <nav className="flex shrink-0 items-center gap-0.5 sm:gap-1.5" aria-label={t('landing.nav.label')}>
          <Button as={RouterLink} to="/models" isIconOnly className="md:!w-auto md:!min-w-0 md:!gap-2 md:!px-2" size="sm" title={t('landing.nav.models')} aria-label={t('landing.nav.models')} variant="light">
            <Boxes className="size-4" aria-hidden="true" />
            <span className="hidden md:inline">{t('landing.nav.models')}</span>
          </Button>
          <Button as={RouterLink} to="/api" isIconOnly className="md:!w-auto md:!min-w-0 md:!gap-2 md:!px-2" size="sm" title={t('apiExplorer.nav')} aria-label={t('apiExplorer.nav')} variant="light">
            <Braces className="size-4" aria-hidden="true" />
            <span className="hidden md:inline">API</span>
          </Button>
          <span className="mx-1 hidden h-5 w-px bg-[var(--hairline)] md:block" aria-hidden="true" />
          <div className="hidden items-center gap-1.5 md:flex">
            <LanguageToggle />
            <MotionToggle />
            <ThemeToggle />
          </div>
          <Button as={RouterLink} to="/console" isIconOnly className="md:!w-auto md:!min-w-0 md:!px-3" color="primary" size="sm" title={t('landing.nav.console')} aria-label={t('landing.nav.console')}>
            <LayoutDashboard className="size-4 md:hidden" aria-hidden="true" />
            <span className="hidden md:inline">{t('landing.nav.console')}</span>
          </Button>
          <Popover placement="bottom-end">
            <PopoverTrigger>
              <Button isIconOnly className="md:hidden" size="sm" title={t('nav.settings')} aria-label={t('nav.settings')} variant="light">
                <SlidersHorizontal className="size-4" aria-hidden="true" />
              </Button>
            </PopoverTrigger>
            <PopoverContent className="max-w-[calc(100vw-1.5rem)] rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
              <div className="flex items-center gap-2">
                <LanguageToggle />
                <MotionToggle />
                <ThemeToggle />
              </div>
            </PopoverContent>
          </Popover>
        </nav>
      </div>
    </header>
  )
}
