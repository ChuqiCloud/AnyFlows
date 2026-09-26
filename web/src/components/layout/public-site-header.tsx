import { Boxes, Braces, Menu } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LanguageToggle } from '@/components/language-toggle'
import { MotionToggle } from '@/components/motion-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import { Button } from '@/components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import type { SiteNavigation } from '@/lib/api/generated/types.gen'

type PublicSiteHeaderProps = {
  logoUrl?: string | null
  siteName: string
  navigation?: SiteNavigation
}

/** 公开页面共用同一站点身份与导航，避免跨页面时顶栏结构和操作入口跳变。 */
export function PublicSiteHeader({ logoUrl, siteName, navigation }: PublicSiteHeaderProps) {
  const { i18n, t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const links = (navigation ?? siteQuery.data?.navigation)?.header_links ?? []
  const label = (link: typeof links[number]) => i18n.resolvedLanguage?.startsWith('en') && link.label_en ? link.label_en : link.label

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
          {links.slice(0, 2).map((link) => (
            <Button key={`${link.url}:${link.label}`} variant="ghost" size="sm" className="hidden max-w-28 xl:inline-flex" asChild>
              <a href={link.url.startsWith('/') ? `#${link.url}` : link.url} target={link.url.startsWith('/') ? undefined : '_blank'} rel={link.url.startsWith('/') ? undefined : 'noopener noreferrer'} className="truncate">{label(link)}</a>
            </Button>
          ))}
          {links.length > 0 && <Popover>
            <PopoverTrigger asChild><Button variant="ghost" size="icon-sm" title={t('nav.customLinks')} aria-label={t('nav.customLinks')}><Menu className="size-4" /></Button></PopoverTrigger>
            <PopoverContent align="end" className="w-52 max-w-[calc(100vw-1rem)] p-1.5">
              <nav aria-label={t('nav.customLinks')} className="grid gap-0.5">
                {links.map((link) => <a key={`${link.url}:${link.label}`} href={link.url.startsWith('/') ? `#${link.url}` : link.url} target={link.url.startsWith('/') ? undefined : '_blank'} rel={link.url.startsWith('/') ? undefined : 'noopener noreferrer'} className="truncate rounded-md px-3 py-2 text-sm hover:bg-surface-2">{label(link)}</a>)}
              </nav>
            </PopoverContent>
          </Popover>}
          <Button variant="ghost" size="sm" asChild>
            <a href="#/api" title={t('apiExplorer.nav')}>
              <Braces className="size-4" aria-hidden="true" />
              <span className="hidden md:inline">API</span>
              <span className="sr-only md:hidden">API</span>
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
