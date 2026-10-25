import { useTranslation } from 'react-i18next'

import { SoftwareUpdatePanel } from '@/features/site-settings/software-update-panel'

const partners = [
  {
    name: '初七云',
    href: 'https://chuqiyun.com/',
    logo: '/assets/sponsors/chuqicloud.png',
    descriptionKey: 'about.sponsors.chuqiCloud',
    descriptionFallback: '专注云计算服务，为 AnyFlows 提供云计算资源支持。',
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
    <div className="grid gap-6">
      <header className="grid gap-2">
        <p className="text-xs font-medium uppercase text-muted-foreground">AnyFlows</p>
        <h2 className="text-lg font-semibold">{t('about.companyName')}</h2>
        <p className="max-w-2xl text-sm leading-6 text-muted-foreground">{t('about.description')}</p>
      </header>

      <section aria-labelledby="about-partners-title" className="grid gap-3">
        <div>
          <h3 id="about-partners-title" className="text-sm font-semibold">{t('about.partnersTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('about.partnersDescription')}</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          {partners.map((partner) => (
            <a
              key={partner.name}
              href={partner.href}
              target="_blank"
              rel="noopener noreferrer"
              className="flex min-h-36 flex-col items-center justify-center gap-3 rounded-lg border border-[var(--hairline)] p-4 text-center transition-colors hover:bg-muted/40"
            >
              <span className="flex h-12 max-w-full items-center justify-center">
                {partner.darkLogo ? <img src={partner.darkLogo} alt={partner.name} className="hidden max-h-full max-w-full object-contain dark:block" /> : null}
                <img src={partner.logo} alt={partner.name} className={`max-h-full max-w-full object-contain ${partner.darkLogo ? 'dark:hidden' : ''}`} />
              </span>
              <span className="grid gap-1">
                <span className="text-sm font-medium">{partner.name}</span>
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
