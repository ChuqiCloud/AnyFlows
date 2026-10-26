import { Button } from '@heroui/react'
import {
  Activity,
  ArrowRight,
  Boxes,
  Braces,
  Check,
  CircleDollarSign,
  Cpu,
  GitBranch,
  GitMerge,
  HeartPulse,
  KeyRound,
  Layers3,
  Network,
  Route,
  Server,
  ShieldCheck,
  Sparkles,
  Zap,
} from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { BorderBeam } from '@/components/brand/border-beam'
import { RibbonBeam } from '@/components/brand/ribbon-beam'
import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { AnnouncementStrip } from '@/features/announcements/announcement-strip'
import { DeveloperTestimonials } from '@/features/landing/developer-testimonials'
import { FaqAccordion } from '@/features/landing/faq-accordion'
import { FeaturedModelsTable } from '@/features/landing/featured-models-table'
import { HeroTerminal } from '@/features/landing/hero-terminal'
import { HeroTicker } from '@/features/landing/hero-ticker'
import { QuickStartSteps } from '@/features/landing/quick-start-steps'
import { TransparencyBento } from '@/features/landing/transparency-bento'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { useLandingModels } from './use-landing-models'
import { cn } from '@/lib/utils'

type JourneyMode = 'healthy' | 'failover' | 'streaming'
type JourneyTone = 'billing' | 'health' | 'observability' | 'streaming' | 'pool'

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
    <div className="next-landing-flow relative mt-5 overflow-hidden rounded-xl p-4">
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
  badge,
}: {
  icon: typeof Activity
  tone: JourneyTone
  title: string
  children: string
  badge?: string
}) {
  return (
    <article className={`next-landing-card next-landing-card-${tone} h-full`}>
      <div className="next-landing-card-content">
        <div className="flex items-center justify-between">
          <div className="next-landing-card-icon">
            <Icon className="size-4" aria-hidden="true" />
          </div>
          {badge && (
            <span className="rounded-full bg-surface-2 px-2 py-0.5 text-[0.625rem] font-medium text-muted-foreground">
              {badge}
            </span>
          )}
        </div>
        <h3 className="mt-4 text-base font-semibold tracking-normal text-foreground">{title}</h3>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">{children}</p>
      </div>
      <div className={`next-landing-card-visual next-landing-card-visual-${tone}`} aria-hidden="true">
        {tone === 'health' ? (
          <svg viewBox="0 0 160 42" preserveAspectRatio="none">
            <path d="M0 25 H34 L44 25 L53 10 L62 35 L73 18 L80 25 H160" />
          </svg>
        ) : tone === 'observability' ? (
          <>
            <span />
            <span />
            <span />
          </>
        ) : tone === 'pool' ? (
          <div className="flex w-full items-center justify-around">
            <span className="size-2 rounded-full bg-current animate-ping" />
            <span className="size-2 rounded-full bg-current" />
            <span className="size-2 rounded-full bg-current opacity-60" />
            <span className="size-2 rounded-full bg-current opacity-40" />
          </div>
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
  const { authenticated: hasSession, models } = useLandingModels()
  const sampleModel = models.find((model) => model.output_modalities.includes('text'))?.model ?? 'YOUR_MODEL_ID'
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
    <div className="min-h-dvh overflow-hidden bg-background text-foreground antialiased selection:bg-primary/20 selection:text-primary">
      <PublicSiteHeader siteName={siteName} logoUrl={site?.brand.logo_url} />
      <div className="pt-14">
        <AnnouncementStrip />
      </div>

      <main>
        {/* ===================== HERO SECTION ===================== */}
        <section className="relative isolate overflow-hidden pt-8 pb-12 sm:pt-14 sm:pb-20 lg:pt-18 lg:pb-24">
          {/* Ambient Lighting & Ribbon Beam */}
          <RibbonBeam className="opacity-70 dark:opacity-90" />
          <div className="next-landing-grid pointer-events-none absolute inset-0 -z-20 opacity-50 dark:opacity-30" aria-hidden="true" />
          <div className="next-landing-hero-light pointer-events-none absolute inset-0 -z-10" aria-hidden="true" />

          <div className="mx-auto grid w-full max-w-[1280px] items-center gap-10 px-4 sm:gap-14 sm:px-6 lg:grid-cols-[1.08fr_0.92fr] lg:gap-14 lg:px-8">
            {/* Left Column: Headline, Description, CTAs & 3 Pillars */}
            <div className="next-landing-rise flex flex-col justify-center">
              <div className="inline-flex w-fit items-center gap-2 rounded-full border border-primary/25 bg-primary/10 px-3.5 py-1 text-xs font-mono font-medium text-primary shadow-xs tracking-wider uppercase">
                <Sparkles className="size-3.5 text-primary" aria-hidden="true" />
                <span>{t('landing.hero.eyebrow')}</span>
              </div>

              <h1 className="mt-5 max-w-2xl text-[2.75rem] font-semibold leading-[1.06] tracking-[-0.04em] sm:mt-6 sm:text-6xl lg:text-6.5xl text-foreground">
                {site?.brand.tagline || t('landing.hero.headline')}
                <span className="block mt-1 bg-gradient-to-r from-primary via-brand to-info bg-clip-text text-transparent">
                  {t('landing.hero.headlineAccent')}
                </span>
              </h1>

              <p className="mt-5 max-w-xl text-base leading-7 text-muted-foreground sm:mt-6 sm:text-lg sm:leading-8">
                {site?.brand.description || t('landing.hero.siteDescription')}
              </p>

              {/* Action Buttons */}
              <div className="mt-7 flex flex-wrap items-center gap-3 sm:mt-8">
                <Button
                  as={RouterLink}
                  color="primary"
                  size="lg"
                  to={accountPath}
                  className="font-medium shadow-md shadow-primary/20 hover:shadow-lg hover:shadow-primary/30 transition-shadow rounded-full px-6"
                  endContent={<ArrowRight className="size-4" aria-hidden="true" />}
                >
                  {accountCtaLabel}
                </Button>
                <Button
                  as={RouterLink}
                  size="lg"
                  to="/models"
                  variant="flat"
                  startContent={<Boxes className="size-4 text-muted-foreground" aria-hidden="true" />}
                  className="border border-[var(--hairline)] bg-surface-1/80 hover:bg-surface-2 rounded-full px-5"
                >
                  {t('landing.nav.models')}
                </Button>
                <Button
                  as={RouterLink}
                  size="lg"
                  to="/api"
                  variant="light"
                  startContent={<Braces className="size-4 text-muted-foreground" aria-hidden="true" />}
                  className="text-muted-foreground hover:text-foreground rounded-full"
                >
                  {t('landing.nav.api')}
                </Button>
              </div>

              {/* Trust Badges */}
              <div className="mt-6 flex flex-wrap items-center gap-x-6 gap-y-2.5 text-xs text-muted-foreground sm:mt-7">
                <span className="inline-flex items-center gap-1.5 font-medium text-foreground/85">
                  <Check className="size-3.5 text-success font-bold" aria-hidden="true" />
                  {t('landing.home.heroChecks.compatible')}
                </span>
                <span className="inline-flex items-center gap-1.5 font-medium text-foreground/85">
                  <Check className="size-3.5 text-success font-bold" aria-hidden="true" />
                  {t('landing.home.heroChecks.health')}
                </span>
                <span className="inline-flex items-center gap-1.5 font-medium text-foreground/85">
                  <Check className="size-3.5 text-success font-bold" aria-hidden="true" />
                  {t('landing.home.heroChecks.logs')}
                </span>
              </div>

              {/* Architectural 3 Pillars (Sidrune hfeat inspiration) */}
              <div className="mt-9 grid grid-cols-1 sm:grid-cols-3 border-t border-[var(--hairline)] pt-5 gap-4">
                <div className="sm:pr-4">
                  <div className="font-mono text-[0.6875rem] font-semibold text-primary tracking-wider">
                    01 / {t('landing.pageUi.protocolLabel')}
                  </div>
                  <div className="mt-1 text-xs font-semibold text-foreground">{t('landing.pageUi.protocolTitle')}</div>
                  <div className="mt-1 text-[0.6875rem] leading-normal text-muted-foreground">
                    {t('landing.pageUi.protocolDescription')}
                  </div>
                </div>
                <div className="sm:px-4 sm:border-l border-[var(--hairline)]">
                  <div className="font-mono text-[0.6875rem] font-semibold text-success tracking-wider">
                    02 / {t('landing.pageUi.routingLabel')}
                  </div>
                  <div className="mt-1 text-xs font-semibold text-foreground">{t('landing.pageUi.routingTitle')}</div>
                  <div className="mt-1 text-[0.6875rem] leading-normal text-muted-foreground">
                    {t('landing.pageUi.routingDescription')}
                  </div>
                </div>
                <div className="sm:pl-4 sm:border-l border-[var(--hairline)]">
                  <div className="font-mono text-[0.6875rem] font-semibold text-info tracking-wider">
                    03 / {t('landing.pageUi.billingLabel')}
                  </div>
                  <div className="mt-1 text-xs font-semibold text-foreground">{t('landing.pageUi.billingTitle')}</div>
                  <div className="mt-1 text-[0.6875rem] leading-normal text-muted-foreground">
                    {t('landing.pageUi.billingDescription')}
                  </div>
                </div>
              </div>
            </div>

            {/* Right Column: Hero Terminal Sandbox */}
            <div className="next-landing-rise next-landing-rise-delay">
              <HeroTerminal />
            </div>
          </div>
        </section>

        {/* ===================== HERO STATUS TICKER ===================== */}
        <HeroTicker />

        {/* ===================== FEATURED MODELS & LIVE PRICING MATRIX ===================== */}
        <FeaturedModelsTable />

        {/* ===================== QUICK START: 4 STEPS ===================== */}
        <QuickStartSteps />

        {/* ===================== THREE GATEWAY MODES ===================== */}
        <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
          <div className="max-w-2xl">
            <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
              <Network className="size-3.5" />
              <span>{t('landing.modes.eyebrow')}</span>
            </div>
            <h2 className="mt-3 text-3xl font-bold tracking-tight sm:text-4xl text-foreground">
              {t('landing.modes.heading')}
            </h2>
            <p className="mt-2.5 text-base leading-7 text-muted-foreground">
              {t('landing.modes.subheading')}
            </p>
          </div>

          <div className="mt-12 grid gap-6 md:grid-cols-3">
            {/* Mode 1: Relay */}
            <div className="relative rounded-2xl border border-[var(--hairline)] bg-surface-1/80 p-6 shadow-xs transition-all duration-300 hover:-translate-y-1 hover:border-primary/40 hover:shadow-md">
              <div className="size-10 rounded-xl bg-primary/15 text-primary flex items-center justify-center">
                <KeyRound className="size-5" />
              </div>
              <span className="mt-4 inline-block text-xs font-semibold text-primary font-mono tracking-wider">
                {t('landing.modes.relay.label')}
              </span>
              <h3 className="mt-1 text-lg font-semibold text-foreground">
                {t('landing.modes.relay.title')}
              </h3>
              <p className="mt-2.5 text-sm leading-6 text-muted-foreground">
                {t('landing.modes.relay.body')}
              </p>
              <div className="mt-5 flex flex-wrap gap-1.5 pt-4 border-t border-[var(--hairline)]">
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.openAiKey')}
                </span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.anthropicKey')}
                </span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.groupMultipliers')}
                </span>
              </div>
            </div>

            {/* Mode 2: Sub2API (RelaxyCode specialty) */}
            <div className="relative rounded-2xl border border-primary/30 bg-gradient-to-b from-primary/8 via-surface-1 to-surface-1 p-6 shadow-sm transition-all duration-300 hover:-translate-y-1 hover:border-primary/60 hover:shadow-md">
              <div className="size-10 rounded-xl bg-brand/20 text-brand flex items-center justify-center">
                <Cpu className="size-5" />
              </div>
              <span className="mt-4 inline-block text-xs font-semibold text-brand font-mono tracking-wider">
                {t('landing.modes.sub2api.label')}
              </span>
              <h3 className="mt-1 text-lg font-semibold text-foreground">
                {t('landing.modes.sub2api.title')}
              </h3>
              <p className="mt-2.5 text-sm leading-6 text-muted-foreground">
                {t('landing.modes.sub2api.body')}
              </p>
              <div className="mt-5 flex flex-wrap gap-1.5 pt-4 border-t border-[var(--hairline)]">
                <span className="rounded bg-brand/15 px-2 py-0.5 text-[0.6875rem] text-brand font-medium">
                  Claude Code
                </span>
                <span className="rounded bg-brand/15 px-2 py-0.5 text-[0.6875rem] text-brand font-medium">
                  Codex
                </span>
                <span className="rounded bg-brand/15 px-2 py-0.5 text-[0.6875rem] text-brand font-medium">
                  Gemini CLI
                </span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground">
                  {t('landing.pageUi.credentialPool')}
                </span>
              </div>
            </div>

            {/* Mode 3: MetAPI */}
            <div className="relative rounded-2xl border border-[var(--hairline)] bg-surface-1/80 p-6 shadow-xs transition-all duration-300 hover:-translate-y-1 hover:border-primary/40 hover:shadow-md">
              <div className="size-10 rounded-xl bg-info/15 text-info flex items-center justify-center">
                <GitMerge className="size-5" />
              </div>
              <span className="mt-4 inline-block text-xs font-semibold text-info font-mono tracking-wider">
                {t('landing.modes.metapi.label')}
              </span>
              <h3 className="mt-1 text-lg font-semibold text-foreground">
                {t('landing.modes.metapi.title')}
              </h3>
              <p className="mt-2.5 text-sm leading-6 text-muted-foreground">
                {t('landing.modes.metapi.body')}
              </p>
              <div className="mt-5 flex flex-wrap gap-1.5 pt-4 border-t border-[var(--hairline)]">
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.crossProviderFallback')}
                </span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.routingPolicies')}
                </span>
                <span className="rounded bg-surface-2 px-2 py-0.5 text-[0.6875rem] text-muted-foreground font-mono">
                  {t('landing.pageUi.retryStrategies')}
                </span>
              </div>
            </div>
          </div>
        </section>

        {/* ===================== BENTO GRID: WHY ANYFLOWS ===================== */}
        <section className="border-t border-[var(--hairline)] bg-surface-sunken/40 py-20 sm:py-24">
          <div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8">
            <div className="max-w-2xl">
              <p className="font-mono text-xs font-semibold tracking-[0.2em] text-primary uppercase">
                {t('landing.feature.eyebrow')}
              </p>
              <h2 className="mt-3 text-3xl font-bold tracking-tight sm:text-4xl text-foreground">
                {t('landing.pageUi.featuresTitle')}
              </h2>
              <p className="mt-2.5 text-base leading-7 text-muted-foreground">
                {t('landing.feature.subheading')}
              </p>
            </div>

            <div className="mt-12 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {/* Card 1: Intelligent Routing (Featured large card) */}
              <article className="next-landing-route-card relative overflow-hidden rounded-xl border border-[var(--hairline)] p-6 sm:col-span-2">
                <div className="relative z-10 max-w-lg">
                  <div className="next-landing-card-icon next-landing-card-icon-primary">
                    <Route className="size-4" aria-hidden="true" />
                  </div>
                  <h3 className="mt-4 text-xl font-semibold tracking-normal text-foreground">
                    {t('landing.feature.routing.title')}
                  </h3>
                  <p className="mt-2.5 text-sm leading-6 text-muted-foreground">
                    {t('landing.feature.routing.body')}
                  </p>
                </div>
                <div className="next-landing-route-map mt-6" aria-hidden="true">
                  <div className="next-landing-route-origin"><Braces className="size-4" /></div>
                  <div className="next-landing-route-tracks"><span /><span /><span /></div>
                  <div className="next-landing-route-destinations"><span /><span /><span /></div>
                </div>
                <div className="relative z-10 mt-6 flex flex-wrap gap-2 text-xs text-muted-foreground font-medium">
                  <span className="rounded-full bg-surface-2 px-2.5 py-1">{t('landing.pageUi.weightedRouting')}</span>
                  <span className="rounded-full bg-surface-2 px-2.5 py-1">{t('landing.pageUi.healthProbes')}</span>
                  <span className="rounded-full bg-surface-2 px-2.5 py-1">{t('landing.pageUi.configuredFallback')}</span>
                </div>
              </article>

              {/* Card 2: Billing & Transparency */}
              <JourneyCard
                icon={CircleDollarSign}
                tone="billing"
                badge={t('landing.pageUi.badges.billing')}
                title={t('landing.feature.billing.title')}
              >
                {t('landing.feature.billing.body')}
              </JourneyCard>

              {/* Card 3: Health Probing */}
              <JourneyCard
                icon={HeartPulse}
                tone="health"
                badge={t('landing.pageUi.badges.health')}
                title={t('landing.feature.health.title')}
              >
                {t('landing.feature.health.body')}
              </JourneyCard>

              {/* Card 4: Observability & Traces */}
              <JourneyCard
                icon={ShieldCheck}
                tone="observability"
                badge={t('landing.pageUi.badges.observability')}
                title={t('landing.feature.observability.title')}
              >
                {t('landing.feature.observability.body')}
              </JourneyCard>

              {/* Card 5: High-speed Streaming */}
              <JourneyCard
                icon={Layers3}
                tone="streaming"
                badge={t('landing.pageUi.badges.streaming')}
                title={t('landing.feature.streaming.title')}
              >
                {t('landing.feature.streaming.body')}
              </JourneyCard>

              {/* Card 6: Account Pools & Proxies */}
              <JourneyCard
                icon={Server}
                tone="pool"
                badge={t('landing.pageUi.badges.pool')}
                title={t('landing.feature.pool.title')}
              >
                {t('landing.feature.pool.body')}
              </JourneyCard>
            </div>
          </div>
        </section>

        {/* ===================== LIVE REQUEST JOURNEY & DEMO ===================== */}
        <section className="border-t border-[var(--hairline)] bg-background">
          <div className="mx-auto grid w-full max-w-[1240px] gap-10 px-4 py-20 sm:px-6 sm:py-24 lg:grid-cols-[0.95fr_1.05fr] lg:items-center lg:px-8">
            <div>
              <p className="font-mono text-xs font-semibold tracking-[0.2em] text-primary uppercase">
                {t('landing.home.status.eyebrow')}
              </p>
              <h2 className="mt-3 text-3xl font-bold tracking-tight sm:text-4xl text-foreground">
                {t('landing.home.status.heading')}
              </h2>
              <p className="mt-2.5 max-w-lg text-base leading-7 text-muted-foreground">
                {t('landing.home.status.description')}
              </p>

              {/* Mode Buttons */}
              <div className="mt-8 flex flex-wrap gap-2.5" role="tablist" aria-label={t('landing.home.status.tablist')}>
                {journeyModes.map(({ key, icon: Icon, color }) => (
                  <Button
                    key={key}
                    aria-selected={mode === key}
                    className={cn(
                      'cursor-pointer transition-all rounded-full',
                      mode === key ? 'bg-primary/15 text-primary border border-primary/30 font-semibold' : 'bg-surface-1 border border-[var(--hairline)]',
                    )}
                    color={mode === key ? 'primary' : 'default'}
                    size="sm"
                    startContent={<Icon className={`size-4 ${mode === key ? '' : color}`} aria-hidden="true" />}
                    variant="flat"
                    onPress={() => setMode(key)}
                  >
                    {t(`landing.journey.${key}.label`)}
                  </Button>
                ))}
              </div>

              {/* Animated Journey Flow SVG */}
              <RequestFlow mode={mode} />
            </div>

            {/* Live Terminal & Metric Stats */}
            <div className="next-landing-demo rounded-2xl border border-[var(--hairline)] bg-surface-1 p-5 shadow-lg sm:p-6">
              <div className="flex items-center justify-between border-b border-[var(--hairline)] pb-3 text-xs">
                <div className="flex items-center gap-2">
                  <ModeIcon className="size-4 text-primary" aria-hidden="true" />
                  <span className="font-mono text-muted-foreground">POST /v1/chat/completions</span>
                </div>
                <span className="inline-flex items-center gap-1.5 rounded-full bg-success/15 px-2.5 py-0.5 text-xs font-semibold text-success font-mono">
                  <span className="size-1.5 rounded-full bg-success animate-pulse" />
                  {t('landing.catalog.example')}
                </span>
              </div>

              <div className="mt-4 grid gap-4 sm:grid-cols-[1fr_0.8fr]">
                <div className="rounded-xl bg-surface-sunken p-4 font-mono text-xs leading-6 text-muted-foreground border border-[var(--hairline)]">
                  <span className="text-primary">{`{`}</span><br />
                  &nbsp;&nbsp;<span className="text-info">"model"</span>: <span className="text-success">{JSON.stringify(sampleModel)}</span>,<br />
                  &nbsp;&nbsp;<span className="text-info">"stream"</span>: <span className="text-warning">true</span>,<br />
                  &nbsp;&nbsp;<span className="text-info">"max_tokens"</span>: <span className="text-warning">4096</span><br />
                  <span className="text-primary">{`}`}</span>
                </div>

                <div className="grid gap-2.5">
                  <div className="rounded-xl bg-primary/10 p-3 border border-primary/20">
                    <p className="text-xs text-muted-foreground">{t('landing.home.status.route')}</p>
                    <p className="mt-1 text-sm font-semibold text-foreground">
                      {t(`landing.journey.${mode}.route`)}
                    </p>
                  </div>
                  <div className="rounded-xl bg-success/10 p-3 border border-success/20">
                    <p className="text-xs text-muted-foreground">{t('landing.pageUi.exampleLabel')}</p>
                    <p className="mt-1 text-sm font-semibold text-success font-mono">
                      {t('landing.catalog.example')}
                    </p>
                  </div>
                  <div className="rounded-xl bg-warning/10 p-3 border border-warning/20">
                    <p className="text-xs text-muted-foreground">{t('landing.pageUi.requestTrace')}</p>
                    <p className="mt-1 text-sm font-semibold text-warning font-mono">
                      {t('landing.pageUi.requestTraceValue')}
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* ===================== TRANSPARENCY & OBSERVABILITY BENTO ===================== */}
        <TransparencyBento />

        {/* ===================== DEVELOPER TESTIMONIALS ===================== */}
        <DeveloperTestimonials />

        {/* ===================== FAQ ACCORDION ===================== */}
        <FaqAccordion />

        {/* ===================== CALL TO ACTION (CTA) BANNER ===================== */}
        <section className="relative overflow-hidden border-t border-[var(--hairline)] py-20 sm:py-28 bg-gradient-to-b from-background via-surface-1 to-surface-sunken">
          <div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8">
            <div className="relative rounded-3xl border border-[var(--hairline)] bg-surface-1 p-8 sm:p-12 lg:p-16 shadow-2xl overflow-hidden">
              <BorderBeam radius={24} borderWidth={2} beamSize={35} />

              <div className="relative z-10 max-w-2xl">
                <div className="inline-flex items-center gap-1.5 rounded-full border border-primary/25 bg-primary/10 px-3 py-1 text-xs font-mono font-medium text-primary uppercase tracking-wider">
                  <Sparkles className="size-3.5" />
                  <span>{t('landing.pageUi.ctaBadge')}</span>
                </div>

                <h2 className="mt-4 text-3xl font-bold tracking-tight text-foreground sm:text-4xl lg:text-5xl">
                  {t('landing.pageUi.ctaTitle')}
                </h2>

                <p className="mt-4 text-base leading-7 text-muted-foreground sm:text-lg">
                  {t('landing.pageUi.ctaDescription')}
                </p>

                <div className="mt-8 flex flex-wrap gap-3">
                  <Button
                    as={RouterLink}
                    color="primary"
                    size="lg"
                    to={accountPath}
                    className="font-medium shadow-md shadow-primary/25 hover:shadow-lg transition-shadow rounded-full px-6"
                    endContent={<ArrowRight className="size-4" aria-hidden="true" />}
                  >
                    {accountCtaLabel}
                  </Button>
                  <Button
                    as={RouterLink}
                    size="lg"
                    to="/models"
                    variant="flat"
                    className="border border-[var(--hairline)] bg-surface-2 rounded-full px-5"
                  >
                    {t('landing.nav.models')}
                  </Button>
                  <Button
                    as={RouterLink}
                    size="lg"
                    to="/api"
                    variant="light"
                    className="text-muted-foreground hover:text-foreground rounded-full"
                  >
                    {t('landing.nav.api')}
                  </Button>
                </div>

                <div className="mt-8 flex flex-wrap items-center gap-x-6 gap-y-2 text-xs text-muted-foreground">
                  <span>{t('landing.pageUi.clientExamples')}</span>
                  <span>{t('landing.pageUi.sdkCompatibility')}</span>
                  <span>{t('landing.pageUi.routingRetries')}</span>
                </div>
              </div>
            </div>
          </div>
        </section>
      </main>

      <SiteFooter siteName={siteName} logoUrl={site?.brand.logo_url} navigation={site?.navigation} />
    </div>
  )
}
