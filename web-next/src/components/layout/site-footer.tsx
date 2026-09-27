import { ArrowUpRight, Braces, Boxes, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'
import type { SiteNavigation } from '@/lib/api/generated/types.gen'

type SiteFooterProps = { siteName: string; logoUrl?: string | null; navigation?: SiteNavigation }

export function SiteFooter({ siteName, logoUrl, navigation }: SiteFooterProps) {
  const { i18n, t } = useTranslation()
  const english = i18n.resolvedLanguage?.startsWith('en')
  const groups = navigation?.footer_groups ?? []

  return <footer className="border-t border-[var(--hairline)] bg-surface-sunken/70 px-4 pt-14 pb-6 sm:px-6 lg:px-8">
    <div className="mx-auto grid w-full max-w-[1240px] gap-10 border-b border-[var(--hairline)] pb-12 md:grid-cols-[minmax(0,1.5fr)_repeat(2,minmax(0,1fr))] lg:grid-cols-[minmax(0,1.5fr)_repeat(3,minmax(0,1fr))]">
      <div className="max-w-sm"><div className="flex items-center gap-3"><SiteBrandMark siteName={siteName} logoUrl={logoUrl} /><strong className="truncate text-base">{siteName}</strong></div><p className="mt-4 text-sm leading-6 text-muted-foreground">{t('brand.scope')}</p><span className="mt-5 inline-flex items-center gap-1.5 text-xs text-primary"><Zap className="size-3.5" />API Platform</span></div>
      <div><h2 className="mb-4 text-xs font-semibold text-foreground">{t('nav.label')}</h2><nav className="grid gap-3 text-sm text-muted-foreground"><RouterLink to="/models" className="inline-flex items-center gap-2 hover:text-primary"><Boxes className="size-3.5" />{t('landing.nav.models')}</RouterLink><RouterLink to="/api" className="inline-flex items-center gap-2 hover:text-primary"><Braces className="size-3.5" />API</RouterLink></nav></div>
      {groups.map((group, index) => <div key={`${group.title}:${index}`}><h2 className="mb-4 text-xs font-semibold text-foreground">{english && group.title_en ? group.title_en : group.title}</h2><nav className="grid gap-3 text-sm text-muted-foreground">{group.links.map((link) => link.url.startsWith('/') ? <RouterLink key={`${link.url}:${link.label}`} to={link.url} className="truncate hover:text-primary">{english && link.label_en ? link.label_en : link.label}</RouterLink> : <a key={`${link.url}:${link.label}`} href={link.url} target="_blank" rel="noopener noreferrer" className="inline-flex min-w-0 items-center gap-1.5 hover:text-primary"><span className="truncate">{english && link.label_en ? link.label_en : link.label}</span><ArrowUpRight className="size-3 shrink-0" /></a>)}</nav></div>)}
    </div>
    <div className="mx-auto flex w-full max-w-[1240px] flex-wrap items-center justify-between gap-3 pt-5 text-xs text-muted-foreground"><span>{t('landing.footer.copyright')}</span><span>{siteName}</span></div>
  </footer>
}
