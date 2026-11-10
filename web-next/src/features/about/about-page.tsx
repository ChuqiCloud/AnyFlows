import { ArrowUpRight } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { BrandMark } from '@/components/layout/brand-mark'
import { SoftwareUpdatePanel } from '@/features/site-settings/software-update-panel'

const partners = [
  {
    name: '初七云',
    href: 'https://chuqiyun.com/',
    logo: '/assets/sponsors/chuqicloud.png',
    descriptionKey: 'about.sponsors.chuqiCloud',
    descriptionFallback: '专注云计算服务，为 AnyFlows 提供稳定的云计算资源与基础设施支持，协助项目持续开发、部署与运行。',
  },
  {
    name: '幻缘映射（ME Frp）',
    href: 'https://www.mefrp.com/',
    logo: '/assets/sponsors/mefrp-light.svg',
    darkLogo: '/assets/sponsors/mefrp-dark.svg',
    descriptionKey: 'about.sponsors.meFrp',
  },
]

export function AboutPage() {
  const { t } = useTranslation()

  return (
    <div className="mx-auto grid w-full max-w-5xl gap-10 pb-6 sm:gap-12">
      <header className="relative isolate overflow-hidden rounded-3xl border border-[var(--hairline)] bg-card px-6 py-10 text-center shadow-[var(--shadow-subtle)] sm:px-12 sm:py-12">
        <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(ellipse_at_top,var(--brand),transparent_70%)] opacity-[0.07]" />
        <div className="flex items-center justify-center gap-3.5 sm:gap-4">
          <BrandMark size="hero" className="size-12 rounded-2xl sm:size-14" />
          <p className="font-display text-4xl font-semibold tracking-[-0.055em] italic sm:text-5xl">AnyFlows</p>
        </div>
        <h2 className="mx-auto mt-7 max-w-2xl text-lg font-medium leading-relaxed tracking-wide text-balance sm:text-xl">{t('about.companyName')}</h2>
        <p className="mt-2 text-[0.6875rem] font-medium uppercase tracking-[0.24em] text-muted-foreground/80">{t('about.companySubtitle')}</p>
      </header>

      <section aria-labelledby="about-partners-title" className="mx-auto grid w-full max-w-3xl gap-6">
        <div className="text-center">
          <h3 id="about-partners-title" className="text-base font-semibold tracking-wide">{t('about.partnersTitle')}</h3>
          <p className="mx-auto mt-2 max-w-lg text-sm leading-6 text-pretty text-muted-foreground">{t('about.partnersDescription')}</p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2">
          {partners.map((partner) => (
            <a
              key={partner.name}
              href={partner.href}
              target="_blank"
              rel="noopener noreferrer"
              className="group relative flex min-h-48 flex-col items-center justify-center gap-5 rounded-2xl border border-[var(--hairline)] bg-card px-7 py-7 text-center shadow-[var(--shadow-subtle)] transition-[border-color,box-shadow,background-color] hover:border-brand/30 hover:bg-brand/[0.025] hover:shadow-[var(--shadow-sm)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand focus-visible:ring-offset-2 focus-visible:ring-offset-background"
            >
              <ArrowUpRight aria-hidden="true" className="absolute top-4 right-4 size-3.5 text-muted-foreground/60 transition-colors group-hover:text-brand" />
              <span className="flex h-12 w-full max-w-52 items-center justify-center">
                {partner.darkLogo ? <img src={partner.darkLogo} alt="" className="hidden max-h-full max-w-full object-contain dark:block" /> : null}
                <img src={partner.logo} alt="" className={`max-h-full max-w-full object-contain ${partner.darkLogo ? 'dark:hidden' : ''}`} />
              </span>
              <span className="grid gap-2">
                <span className="text-sm font-semibold tracking-wide">{partner.name}</span>
                <span className="text-xs leading-5 text-muted-foreground">{t(partner.descriptionKey, { defaultValue: partner.descriptionFallback })}</span>
              </span>
            </a>
          ))}
        </div>
      </section>

      <SoftwareUpdatePanel />
    </div>
  )
}
