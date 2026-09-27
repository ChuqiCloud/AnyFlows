import { ArrowUpRight, Braces, Boxes } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'
import type { SiteNavigation } from '@/lib/api/generated/types.gen'

type SiteFooterProps = { siteName: string; logoUrl?: string | null; navigation?: SiteNavigation }

export function SiteFooter({ siteName, logoUrl, navigation }: SiteFooterProps) {
  const { i18n, t } = useTranslation()
  const english = i18n.resolvedLanguage?.startsWith('en')
  const groups = navigation?.footer_groups ?? []

  return <footer className="border-t border-[var(--hairline)] bg-surface-1/50 px-5 pt-12 pb-6 sm:px-6">
    <div className="mx-auto grid max-w-[1120px] gap-10 border-b border-[var(--hairline)] pb-12 md:grid-cols-[minmax(0,1.6fr)_repeat(2,minmax(0,1fr))] lg:grid-cols-[minmax(0,1.6fr)_repeat(3,minmax(0,1fr))]">
      <div className="max-w-sm"><div className="flex items-center gap-3"><SiteBrandMark siteName={siteName} logoUrl={logoUrl} /><strong className="truncate text-base">{siteName}</strong></div><p className="mt-4 text-sm leading-6 text-muted-foreground">{t('brand.scope')}</p></div>
      <div><h2 className="mb-4 text-xs font-semibold text-foreground">{t('nav.label')}</h2><nav className="grid gap-3 text-sm text-muted-foreground"><a href="#/models" className="inline-flex items-center gap-2 hover:text-foreground"><Boxes className="size-3.5" />{t('landing.nav.models')}</a><a href="#/api" className="inline-flex items-center gap-2 hover:text-foreground"><Braces className="size-3.5" />API</a></nav></div>
      {groups.map((group, index) => <div key={`${group.title}:${index}`}><h2 className="mb-4 text-xs font-semibold text-foreground">{english && group.title_en ? group.title_en : group.title}</h2><nav className="grid gap-3 text-sm text-muted-foreground">{group.links.map((link) => <a key={`${link.url}:${link.label}`} href={link.url.startsWith('/') ? `#${link.url}` : link.url} target={link.url.startsWith('/') ? undefined : '_blank'} rel={link.url.startsWith('/') ? undefined : 'noopener noreferrer'} className="inline-flex min-w-0 items-center gap-1.5 hover:text-foreground"><span className="truncate">{english && link.label_en ? link.label_en : link.label}</span>{!link.url.startsWith('/') && <ArrowUpRight className="size-3 shrink-0" />}</a>)}</nav></div>)}
    </div>
    <div className="mx-auto flex max-w-[1120px] flex-wrap justify-between gap-3 pt-5 text-xs text-muted-foreground"><span>{t('landing.footer.copyright')}</span><span>{siteName}</span></div>
  </footer>
}
