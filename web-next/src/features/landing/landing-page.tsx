import { Button } from '@heroui/react'
import {
  Activity,
  ArrowRight,
  Braces,
  Check,
  CircleDollarSign,
  GitBranch,
  HeartPulse,
  Layers3,
  Route,
  ShieldCheck,
  Sparkles,
  Zap,
} from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { LogoMarquee } from '@/components/brand/logo-marquee'
import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { AnnouncementStrip } from '@/features/announcements/announcement-strip'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { getManagementSessionToken } from '@/lib/api/session-token'

type JourneyMode = 'healthy' | 'failover' | 'streaming'
type JourneyTone = 'billing' | 'health' | 'observability' | 'streaming'

const journeyModes: Array<{
  key: JourneyMode
  color: string
  icon: typeof Activity
}> = [
  { key: 'healthy', color: 'text-success', icon: Activity },
  { key: 'failover', color: 'text-warning', icon: GitBranch },
  { key: 'streaming', color: 'text-info', icon: Zap },
]

const pathByTarget = [
  'M44 88 C 126 88, 160 28, 268 28',
  'M44 88 C 126 88, 164 88, 268 88',
  'M44 88 C 126 88, 160 148, 268 148',
]

function RequestFlow({ mode }: { mode: JourneyMode }) {
  const { t } = useTranslation()
  const activeTarget = mode === 'failover' ? 2 : mode === 'streaming' ? 0 : 1

  return (
    <div className="next-landing-flow relative mt-5 overflow-hidden rounded-lg p-4">
      <div className="relative h-40 sm:h-60">
        <svg
          className="absolute inset-0 size-full"
          viewBox="0 0 320 176"
          fill="none"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="next-landing-route-gradient" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0" stopColor="var(--brand)" stopOpacity="0.25" />
              <stop offset="0.52" stopColor="var(--brand)" stopOpacity="0.8" />
              <stop offset="1" stopColor="var(--success)" stopOpacity="0.7" />
            </linearGradient>
          </defs>
          {pathByTarget.map((path, index) => (
            <g key={path}>
              <path
                d={path}
                stroke={index === activeTarget ? 'url(#next-landing-route-gradient)' : 'var(--hairline-strong)'}
                strokeWidth={index === activeTarget ? 2 : 1}
                strokeDasharray={index === activeTarget ? undefined : '3 5'}
              />
              {index === activeTarget ? (
                <circle r="3.5" fill="var(--brand)">
                  <animateMotion
                    className="next-landing-flow-motion"
                    dur={mode === 'streaming' ? '1.7s' : '2.8s'}
                    repeatCount="indefinite"
                    path={path}
                  />
                  <animate
                    attributeName="opacity"
                    dur="2.8s"
                    repeatCount="indefinite"
                    values="0.35;1;0.35"
                  />
                </circle>
              ) : null}
            </g>
          ))}
        </svg>

        <div className="absolute left-3 top-1/2 -translate-y-1/2 sm:left-5">
          <div className="next-landing-node next-landing-node-entry">
            <Braces className="size-4" aria-hidden="true" />
            <span>API</span>
          </div>
        </div>

        <div className="absolute right-1 top-0 flex h-full flex-col justify-between py-2 sm:right-3">
          {['OpenAI', 'AnyFlows', 'Gemini'].map((label, index) => (
            <div
              key={label}
              className={`next-landing-node next-landing-node-upstream ${index === activeTarget ? 'next-landing-node-active' : ''}`}
            >
              <span className="size-1.5 rounded-full bg-current" aria-hidden="true" />
              <span>{label}</span>
            </div>
          ))}
        </div>

        <div className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full border border-[var(--hairline)] bg-[var(--surface-1)] px-3 py-1.5 text-[0.6875rem] text-muted-foreground shadow-[var(--shadow-sm)]">
          {t(`landing.home.preview.${mode}`)}
        </div>
      </div>
    </div>
  )
}

function JourneyCard({
  icon: Icon,
  tone,
  title,
  children,
}: {
  icon: typeof Activity
  tone: JourneyTone
  title: string
  children: string
}) {
  return (
    <article className={`next-landing-card next-landing-card-${tone} h-full`}>
      <div className="next-landing-card-content">
        <div className="next-landing-card-icon"><Icon className="size-4" aria-hidden="true" /></div>
        <h3 className="mt-4 text-base font-semibold tracking-normal">{title}</h3>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{children}</p>
      </div>
      <div className={`next-landing-card-visual next-landing-card-visual-${tone}`} aria-hidden="true">
        {tone === 'health' ? (
          <svg viewBox="0 0 160 42" preserveAspectRatio="none"><path d="M0 25 H34 L44 25 L53 10 L62 35 L73 18 L80 25 H160" /></svg>
        ) : tone === 'observability' ? (
          <><span /><span /><span /></>
        ) : (
          Array.from({ length: tone === 'billing' ? 10 : 16 }, (_, index) => <span key={index} />)
        )}
      </div>
    </article>
  )
}

export function LandingPage() {
  const { t } = useTranslation()
  const [mode, setMode] = useState<JourneyMode>('healthy')
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const authentication = site?.authentication
  const hasSession = Boolean(getManagementSessionToken())
  const registrationAvailable = Boolean(
    authentication?.password_login_enabled && authentication.registration_enabled,
  )
  const accountCtaKey = hasSession
    ? 'console'
    : registrationAvailable
      ? 'register'
      : authentication?.password_login_enabled
        ? 'login'
        : 'models'
  const accountPath = accountCtaKey === 'console'
    ? '/console'
    : accountCtaKey === 'register'
      ? '/register'
      : accountCtaKey === 'login'
        ? '/login'
        : '/models'
  const accountCtaLabel = hasSession
    ? t('landing.nav.console')
    : t(`landing.hero.cta.${accountCtaKey}`)
  const ModeIcon = journeyModes.find((item) => item.key === mode)?.icon ?? Activity

  return (
    <div className="min-h-dvh overflow-hidden bg-background text-foreground">
      <PublicSiteHeader siteName={siteName} logoUrl={site?.brand.logo_url} />
      <div className="pt-14">
        <AnnouncementStrip />
      </div>

      <main>
        <section className="next-landing-hero relative isolate overflow-hidden">
          <div className="next-landing-grid pointer-events-none absolute inset-0 -z-20" aria-hidden="true" />
          <div className="next-landing-hero-light pointer-events-none absolute inset-0 -z-10" aria-hidden="true" />

          <div className="mx-auto grid w-full max-w-[1240px] items-center gap-7 px-4 py-8 sm:gap-12 sm:px-6 sm:py-20 lg:grid-cols-[minmax(0,1fr)_minmax(360px,0.82fr)] lg:gap-16 lg:px-8 lg:py-24">
            <div className="next-landing-rise">
              <div className="inline-flex items-center gap-2 rounded-full border border-primary/20 bg-primary/8 px-3 py-1.5 text-xs font-medium text-primary">
                <Sparkles className="size-3.5" aria-hidden="true" />
                {t('landing.hero.eyebrow')}
              </div>
              <h1 className="mt-4 max-w-2xl text-[2.5rem] font-semibold leading-[1.08] tracking-normal sm:mt-6 sm:text-6xl lg:text-7xl">
                {t('landing.hero.heading')}
              </h1>
              <p className="mt-4 max-w-xl text-sm leading-6 text-muted-foreground sm:mt-6 sm:text-lg sm:leading-7">
                {t('landing.hero.subtitle')}
              </p>
              <div className="mt-6 flex flex-wrap gap-2.5 sm:mt-8">
                <Button as={RouterLink} color="primary" size="lg" to={accountPath} endContent={<ArrowRight className="size-4" aria-hidden="true" />}>
                  {accountCtaLabel}
                </Button>
                <Button as={RouterLink} size="lg" to="/models" variant="flat">
                  {t('landing.nav.models')}
                </Button>
              </div>
              <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2 text-xs text-muted-foreground sm:mt-5">
                <span className="inline-flex items-center gap-1.5"><Check className="size-3.5 text-success" aria-hidden="true" />{t('landing.home.heroChecks.compatible')}</span>
                <span className="inline-flex items-center gap-1.5"><Check className="size-3.5 text-success" aria-hidden="true" />{t('landing.home.heroChecks.health')}</span>
                <span className="inline-flex items-center gap-1.5"><Check className="size-3.5 text-success" aria-hidden="true" />{t('landing.home.heroChecks.logs')}</span>
              </div>
            </div>

            <div className="next-landing-rise next-landing-rise-delay">
              <div className="next-landing-preview p-1 sm:p-5">
                <div className="flex items-center justify-between gap-3 text-[0.6875rem] text-muted-foreground">
                  <span className="font-mono tracking-[0.12em]">{t('landing.home.preview.eyebrow')}</span>
                  <span className="inline-flex items-center gap-1.5 text-success"><span className="size-1.5 rounded-full bg-success" />{t(`landing.home.preview.${mode === 'failover' ? 'statusRecovering' : 'statusHealthy'}`)}</span>
                </div>
                <div className="mt-5 flex items-end justify-between gap-4">
                  <div><p className="text-sm text-muted-foreground">{t('landing.home.preview.entry')}</p><p className="mt-1 text-xl font-semibold">{t('landing.home.preview.title')}</p></div>
                  <ModeIcon className="size-6 text-primary" aria-hidden="true" />
                </div>
                <RequestFlow mode={mode} />
                <div className="next-landing-stats mt-4 grid grid-cols-3 gap-2">
                  <div><p className="text-lg font-semibold text-primary">82ms</p><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('landing.home.preview.stats.latency')}</p></div>
                  <div><p className="text-lg font-semibold text-success">99.98%</p><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('landing.home.preview.stats.availability')}</p></div>
                  <div><p className="text-lg font-semibold text-warning">4 {t('landing.home.preview.stats.channelUnit')}</p><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('landing.home.preview.stats.channels')}</p></div>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className="border-y border-[var(--hairline)] bg-surface-1/70 py-6">
          <div className="mx-auto mb-4 max-w-[1240px] px-4 text-center text-[0.6875rem] font-medium tracking-[0.18em] text-muted-foreground/70 uppercase sm:px-6 lg:px-8">{t('landing.providers')}</div>
          <LogoMarquee duration={96} />
        </section>

        <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
          <div className="max-w-2xl">
            <p className="text-xs font-medium tracking-[0.16em] text-primary uppercase">{t('landing.feature.eyebrow')}</p>
            <h2 className="mt-4 text-3xl font-semibold tracking-[-0.04em] sm:text-4xl">{t('landing.home.journey.heading')}</h2>
            <p className="mt-4 text-base leading-7 text-muted-foreground">{t('landing.feature.subheading')}</p>
          </div>

          <div className="mt-10 grid gap-3 sm:grid-cols-2 lg:grid-cols-[1.45fr_1fr_1fr] lg:grid-rows-[1fr_1fr]">
            <article className="next-landing-route-card relative overflow-hidden rounded-lg border border-[var(--hairline)] p-5 sm:col-span-2 sm:p-6 lg:row-span-2 lg:col-span-1">
              <div className="relative z-10 max-w-md"><div className="next-landing-card-icon next-landing-card-icon-primary"><Route className="size-4" aria-hidden="true" /></div><h3 className="mt-5 text-xl font-semibold tracking-normal">{t('landing.feature.routing.title')}</h3><p className="mt-3 text-sm leading-6 text-muted-foreground">{t('landing.feature.routing.body')}</p></div>
              <div className="next-landing-route-map" aria-hidden="true">
                <div className="next-landing-route-origin"><Braces className="size-4" /></div>
                <div className="next-landing-route-tracks"><span /><span /><span /></div>
                <div className="next-landing-route-destinations"><span /><span /><span /></div>
              </div>
              <div className="relative z-10 mt-5 flex flex-wrap gap-2 text-[0.6875rem] text-muted-foreground"><span>{t('landing.home.journey.weight')}</span><span>{t('landing.home.journey.health')}</span><span>{t('landing.home.journey.failover')}</span></div>
            </article>
            <JourneyCard icon={CircleDollarSign} tone="billing" title={t('landing.feature.billing.title')}>{t('landing.feature.billing.body')}</JourneyCard>
            <JourneyCard icon={HeartPulse} tone="health" title={t('landing.feature.health.title')}>{t('landing.feature.health.body')}</JourneyCard>
            <JourneyCard icon={ShieldCheck} tone="observability" title={t('landing.feature.observability.title')}>{t('landing.feature.observability.body')}</JourneyCard>
            <JourneyCard icon={Layers3} tone="streaming" title={t('landing.feature.streaming.title')}>{t('landing.feature.streaming.body')}</JourneyCard>
          </div>
        </section>

        <section className="border-y border-[var(--hairline)] bg-surface-sunken/60">
          <div className="mx-auto grid w-full max-w-[1240px] gap-10 px-4 py-20 sm:px-6 sm:py-24 lg:grid-cols-[0.9fr_1.1fr] lg:items-center lg:px-8">
            <div><p className="text-xs font-medium tracking-[0.16em] text-primary uppercase">{t('landing.home.status.eyebrow')}</p><h2 className="mt-4 text-3xl font-semibold tracking-[-0.04em] sm:text-4xl">{t('landing.home.status.heading')}</h2><p className="mt-4 max-w-lg text-base leading-7 text-muted-foreground">{t('landing.home.status.description')}</p><div className="mt-7 flex flex-wrap gap-2" role="tablist" aria-label={t('landing.home.status.tablist')}><div className="flex flex-wrap gap-2">{journeyModes.map(({ key, icon: Icon, color }) => <Button key={key} aria-selected={mode === key} className={mode === key ? 'bg-primary/12' : ''} color={mode === key ? 'primary' : 'default'} size="sm" startContent={<Icon className={`size-4 ${mode === key ? '' : color}`} aria-hidden="true" />} variant="flat" onPress={() => setMode(key)}>{t(`landing.journey.${key}.label`)}</Button>)}</div></div></div>
            <div className="next-landing-demo rounded-2xl border border-[var(--hairline)] bg-surface-1 p-4 shadow-[var(--shadow-md)] sm:p-5"><div className="flex items-center justify-between border-b border-[var(--hairline)] pb-3 text-xs"><span className="font-mono text-muted-foreground">POST /v1/chat/completions</span><span className="rounded-full bg-success/10 px-2 py-1 text-success">{t('landing.home.status.ok')}</span></div><div className="mt-4 grid gap-3 sm:grid-cols-[1fr_0.72fr]"><div className="rounded-xl bg-surface-sunken p-4 font-mono text-[0.6875rem] leading-6 text-muted-foreground"><span className="text-primary">{`{`}</span><br />&nbsp;&nbsp;<span className="text-info">"model"</span>: <span className="text-success">"deepseek-chat"</span>,<br />&nbsp;&nbsp;<span className="text-info">"stream"</span>: <span className="text-warning">true</span><br /><span className="text-primary">{`}`}</span></div><div className="grid gap-2"><div className="rounded-xl bg-primary/8 p-3"><p className="text-xs text-muted-foreground">{t('landing.home.status.route')}</p><p className="mt-1 text-sm font-semibold">{t(`landing.journey.${mode}.route`)}</p></div><div className="rounded-xl bg-success/8 p-3"><p className="text-xs text-muted-foreground">{t('landing.console.latency')}</p><p className="mt-1 text-sm font-semibold text-success">82ms · {t('landing.home.status.stable')}</p></div><div className="rounded-xl bg-warning/10 p-3"><p className="text-xs text-muted-foreground">{t('landing.console.tokens')}</p><p className="mt-1 text-sm font-semibold text-warning">1,248 / 386</p></div></div></div></div>
          </div>
        </section>

        <section className="next-landing-cta relative overflow-hidden border-t border-[var(--hairline)] py-16 sm:py-20"><div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8"><div className="relative z-10 max-w-2xl"><p className="text-xs font-medium tracking-[0.16em] text-primary uppercase">{t('landing.console.eyebrow')}</p><h2 className="mt-4 text-3xl font-semibold tracking-normal sm:text-4xl">{t('landing.home.cta.heading')}</h2><p className="mt-4 text-base leading-7 text-muted-foreground">{t('landing.console.subheading')}</p><div className="mt-7 flex flex-wrap gap-2.5"><Button as={RouterLink} color="primary" to={accountPath} endContent={<ArrowRight className="size-4" aria-hidden="true" />}>{t('landing.closing.cta')}</Button><Button as={RouterLink} to="/api" variant="flat" startContent={<Braces className="size-4" aria-hidden="true" />}>API</Button></div></div></div></section>
      </main>

      <SiteFooter siteName={siteName} logoUrl={site?.brand.logo_url} navigation={site?.navigation} />
    </div>
  )
}
