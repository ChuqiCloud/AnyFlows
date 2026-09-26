import { Button, Popover, PopoverContent, PopoverTrigger } from '@heroui/react'
import { Boxes, Braces, LayoutDashboard, Menu } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { LanguageToggle } from '@/components/language-toggle'
import { MotionToggle } from '@/components/motion-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
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
      <div className="mx-auto flex h-14 max-w-[1120px] items-center justify-between gap-2 px-3 sm:px-6">
        <RouterLink
          to="/"
          className="flex min-w-0 flex-1 items-center gap-2.5 rounded-lg focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none md:flex-none"
        >
          <SiteBrandMark siteName={siteName} logoUrl={logoUrl} />
          <span className="truncate text-[0.875rem] font-semibold sm:text-[0.9375rem]">
            {siteName}
          </span>
        </RouterLink>
        <nav className="hidden min-w-0 items-center gap-1 md:flex" aria-label={t('landing.nav.label')}>
          <Button as={RouterLink} to="/models" className="!min-w-0 !gap-2 !px-2" size="sm" title={t('landing.nav.models')} variant="light">
            <Boxes className="size-4" aria-hidden="true" />
            <span>{t('landing.nav.models')}</span>
          </Button>
          <Button as={RouterLink} to="/api" className="!min-w-0 !gap-2 !px-2" size="sm" title={t('apiExplorer.nav')} variant="light">
            <Braces className="size-4" aria-hidden="true" />
            <span>API</span>
          </Button>
          {links.slice(0, 2).map((link) => link.url.startsWith('/') ? <Button key={`${link.url}:${link.label}`} as={RouterLink} to={link.url} className="hidden max-w-28 truncate xl:inline-flex" size="sm" variant="light">{label(link)}</Button> : <Button key={`${link.url}:${link.label}`} as="a" href={link.url} target="_blank" rel="noopener noreferrer" className="hidden max-w-28 truncate xl:inline-flex" size="sm" variant="light">{label(link)}</Button>)}
          {links.length > 0 && <Popover placement="bottom-end"><PopoverTrigger><Button isIconOnly size="sm" title={t('nav.customLinks')} aria-label={t('nav.customLinks')} variant="light"><Menu className="size-4" /></Button></PopoverTrigger><PopoverContent className="max-w-[calc(100vw-1rem)] rounded-lg border border-[var(--hairline)] bg-surface-1 p-2"><nav aria-label={t('nav.customLinks')} className="grid min-w-40 gap-1">{links.map((link) => link.url.startsWith('/') ? <RouterLink key={`${link.url}:${link.label}`} to={link.url} className="truncate rounded-md px-3 py-2 text-sm hover:bg-primary/10">{label(link)}</RouterLink> : <a key={`${link.url}:${link.label}`} href={link.url} target="_blank" rel="noopener noreferrer" className="truncate rounded-md px-3 py-2 text-sm hover:bg-primary/10">{label(link)}</a>)}</nav></PopoverContent></Popover>}
          <span className="mx-1 hidden h-5 w-px bg-[var(--hairline)] md:block" aria-hidden="true" />
          <div className="hidden items-center gap-1.5 md:flex">
            <LanguageToggle />
            <MotionToggle />
            <ThemeToggle />
          </div>
          <Button as={RouterLink} to="/console" className="!min-w-0 !px-3" color="primary" size="sm" title={t('landing.nav.console')}>
            <span>{t('landing.nav.console')}</span>
          </Button>
        </nav>
        <Popover placement="bottom-end">
          <PopoverTrigger><Button isIconOnly className="shrink-0 md:hidden" size="sm" title={t('landing.nav.label')} aria-label={t('landing.nav.label')} variant="light"><Menu className="size-5" /></Button></PopoverTrigger>
          <PopoverContent className="w-[min(19rem,calc(100vw-1.5rem))] rounded-lg border border-[var(--hairline)] bg-surface-1 p-2">
            <nav aria-label={t('landing.nav.label')} className="grid gap-1 text-sm">
              <RouterLink to="/models" className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-primary/10"><Boxes className="size-4" />{t('landing.nav.models')}</RouterLink>
              <RouterLink to="/api" className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-primary/10"><Braces className="size-4" />API</RouterLink>
              {links.map((link) => link.url.startsWith('/') ? <RouterLink key={`${link.url}:${link.label}`} to={link.url} className="truncate rounded-md px-3 py-2 hover:bg-primary/10">{label(link)}</RouterLink> : <a key={`${link.url}:${link.label}`} href={link.url} target="_blank" rel="noopener noreferrer" className="truncate rounded-md px-3 py-2 hover:bg-primary/10">{label(link)}</a>)}
              <RouterLink to="/console" className="flex items-center gap-3 rounded-md border-t border-[var(--hairline)] px-3 py-2 font-medium hover:bg-primary/10"><LayoutDashboard className="size-4" />{t('landing.nav.console')}</RouterLink>
            </nav>
            <div className="mt-2 flex items-center gap-2 border-t border-[var(--hairline)] px-2 pt-2"><LanguageToggle /><MotionToggle /><ThemeToggle /></div>
          </PopoverContent>
        </Popover>
      </div>
    </header>
  )
}
