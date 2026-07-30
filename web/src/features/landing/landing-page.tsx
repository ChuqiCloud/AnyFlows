import {
  ArrowRight,
  Boxes,
  Gauge,
  KeyRound,
  Layers,
  Network,
  Radio,
  ShieldCheck,
  Users,
  Workflow,
  Zap,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LogoField } from '@/components/brand/logo-field'
import { LogoMarquee } from '@/components/brand/logo-marquee'
import { ParticleField } from '@/components/brand/particle-field'
import { RibbonBeam } from '@/components/brand/ribbon-beam'
import { KineticText } from '@/components/kinetic/kinetic-text'
import { BrandMark } from '@/components/layout/brand-mark'
import { LanguageToggle } from '@/components/language-toggle'
import { MotionToggle } from '@/components/motion-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import { Button } from '@/components/ui/button'
import { InView } from '@/components/ui/in-view'
import { SpotlightCard } from '@/components/ui/spotlight-card'
import { BillingVisual } from '@/features/landing/billing-visual'
import { ApiConsole } from '@/features/landing/api-console'
import { ProtocolMatrix } from '@/features/landing/protocol-matrix'
import { RouteVisual } from '@/features/landing/route-visual'
import { useRegistrationStatus } from '@/features/registration/registration-api'

/** 三种部署形态，顺序对应 design/00-overview.md 里的场景编号 */
const deployModes = [
  { key: 'relay', icon: KeyRound },
  { key: 'sub2api', icon: Users },
  { key: 'metapi', icon: Network },
] as const

/** routing 与 billing 已在 bento 大格单独呈现，这里只列其余四项 */
const secondaryFeatures = [
  { key: 'health', icon: Gauge },
  { key: 'streaming', icon: Zap },
  { key: 'pool', icon: Radio },
  { key: 'observability', icon: ShieldCheck },
] as const

const openConsole = () => {
  window.location.hash = '#/console'
}

export function LandingPage() {
  const { t } = useTranslation()
  const registrationStatus = useRegistrationStatus()
  const openAccount = () => {
    window.location.hash = registrationStatus.data?.enabled ? '#/register' : '#/console'
  }

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <header className="fixed inset-x-0 top-0 z-50 border-b border-[var(--hairline)] bg-background/80 backdrop-blur-xl">
        <div className="mx-auto flex h-14 max-w-[1120px] items-center justify-between px-3 sm:px-6">
          <div className="flex items-center gap-2.5">
            <BrandMark />
            <span className="hidden text-[0.9375rem] font-semibold sm:inline">
              {t('brand.name')}
            </span>
          </div>
          <div className="flex items-center gap-1.5">
            <Button variant="ghost" size="sm" asChild>
              <a href="#/models" title={t('landing.nav.models')}>
                <Boxes className="size-4" aria-hidden="true" />
                <span className="hidden md:inline">{t('landing.nav.models')}</span>
                <span className="sr-only md:hidden">{t('landing.nav.models')}</span>
              </a>
            </Button>
            <LanguageToggle />
            <MotionToggle />
            <ThemeToggle />
            <Button size="sm" onClick={openConsole}>
              {t('landing.nav.console')}
            </Button>
          </div>
        </div>
      </header>

      <main>
        {/*
          isolate 是必需的：背景层用负 z-index，若本区不建立层叠上下文，
          它们会上浮到根节点并被外层 bg-background 整块盖掉。
        */}
        <section className="relative isolate flex min-h-[calc(100dvh-4.5rem)] items-center justify-center overflow-hidden px-6 pt-14">
          {/* 模型标识星座：常态几乎不可见，鼠标经过时被柔光显影 */}
          <LogoField className="-z-20" />

          {/* 极光：极慢漂移的柔光，给深色画布一点呼吸 */}
          <div
            className="pointer-events-none absolute top-[42%] left-1/2 -z-10 h-[34rem] w-[58rem] max-w-[130vw] -translate-x-1/2 -translate-y-1/2 animate-aurora blur-[110px]"
            style={{
              background:
                'radial-gradient(ellipse 60% 50% at 40% 45%, rgb(255 99 99 / 0.16), transparent 70%),' +
                'radial-gradient(ellipse 55% 45% at 65% 55%, rgb(85 179 255 / 0.12), transparent 70%)',
            }}
            aria-hidden="true"
          />

          {/*
            字标背后压一层底色，保证正文对比度。范围收得比字标略大即可——
            铺满会把整块标识星座盖掉，那是之前背景完全看不见的原因。
          */}
          <div
            className="pointer-events-none absolute inset-0 -z-10"
            style={{
              background:
                'radial-gradient(ellipse 34% 26% at 50% 47%, var(--background) 42%, transparent 76%)',
            }}
            aria-hidden="true"
          />

          {/*
            浮尘层放在字标底色遮罩之后（z 更高），否则中心那块 background
            会把粒子整片盖掉——那是最显眼的区域，粒子恰恰最需要出现在这里。
          */}
          {/*
            绸带压在字标遮罩之下（-z-10 一侧），只让它从字标外侧掠过。
            若提到粒子层那一档，芯线会横切标题，直接毁掉首屏阅读。
          */}
          <RibbonBeam className="-z-[11] opacity-[0.55]" />

          <ParticleField className="-z-[5]" />

          <div className="w-full max-w-[1120px] text-center">
            <p className="text-[0.8125rem] font-medium text-muted-foreground">
              {t('landing.hero.eyebrow')}
            </p>

            {/* 字标即标题，不翻译，保留原始大小写 */}
            <h1 className="mt-6">
              <KineticText
                as="span"
                text={t('brand.name')}
                className="block text-[clamp(3.75rem,13vw,11rem)]"
                delay={0.05}
                stagger={0.04}
                interactive
              />
            </h1>

            <p className="mx-auto mt-7 max-w-lg text-lg leading-relaxed text-muted-foreground">
              {t('landing.hero.subtitle')}
            </p>

            <div className="mt-9 flex flex-wrap items-center justify-center gap-2.5">
              <Button size="lg" onClick={openAccount}>
                {t('landing.hero.cta')}
                <ArrowRight className="size-4" aria-hidden="true" />
              </Button>
              <Button variant="secondary" size="lg">
                {t('landing.hero.secondary')}
              </Button>
            </div>

          </div>
        </section>

        <section className="border-y border-[var(--hairline)] py-8">
          <p className="mb-7 text-center text-xs font-medium tracking-normal text-muted-foreground/70 uppercase">
            {t('landing.providers')}
          </p>
          <LogoMarquee />
        </section>

        <section className="mx-auto max-w-[1120px] px-6 py-32">
          <InView className="mx-auto max-w-2xl text-center">
            <p className="text-xs font-medium tracking-normal text-brand uppercase">
              {t('landing.feature.eyebrow')}
            </p>
            <h2 className="mt-5 text-[clamp(1.875rem,4.5vw,3rem)] leading-[1.1] font-semibold tracking-normal">
              {t('landing.feature.heading')}
            </h2>
            <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-muted-foreground">
              {t('landing.feature.subheading')}
            </p>
          </InView>

          {/*
            Bento 不等宽网格：主特性占两列并配可视化，其余为等宽小格。
            六个等宽格子只有图标+标题+正文时会显得像功能清单而非产品叙事。
          */}
          <div className="mt-20 grid gap-4 lg:grid-cols-3">
            <InView className="lg:col-span-2">
              <SpotlightCard className="h-full rounded-xl border border-[var(--hairline)] p-8">
                <Workflow className="size-5 text-brand" aria-hidden="true" />
                <h3 className="mt-6 text-xl font-semibold tracking-normal">
                  {t('landing.feature.routing.title')}
                </h3>
                <p className="mt-3 max-w-md text-sm leading-relaxed text-muted-foreground">
                  {t('landing.feature.routing.body')}
                </p>
                <RouteVisual />
              </SpotlightCard>
            </InView>

            <InView delay={90}>
              <SpotlightCard className="h-full rounded-xl border border-[var(--hairline)] p-8">
                <Layers className="size-5 text-brand" aria-hidden="true" />
                <h3 className="mt-6 text-xl font-semibold tracking-normal">
                  {t('landing.feature.billing.title')}
                </h3>
                <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                  {t('landing.feature.billing.body')}
                </p>
                <BillingVisual />
              </SpotlightCard>
            </InView>

            {secondaryFeatures.map(({ key, icon: Icon }, index) => (
              /* 递增延迟让四张小卡依次入场，而不是整排同时弹出 */
              <InView key={key} delay={index * 80}>
                <SpotlightCard className="h-full rounded-xl border border-[var(--hairline)] p-8">
                  <Icon className="size-5 text-brand" aria-hidden="true" />
                  <h3 className="mt-6 text-base font-semibold">
                    {t(`landing.feature.${key}.title`)}
                  </h3>
                  <p className="mt-2.5 text-sm leading-relaxed text-muted-foreground">
                    {t(`landing.feature.${key}.body`)}
                  </p>
                </SpotlightCard>
              </InView>
            ))}
          </div>
        </section>

        {/*
          三种部署形态是 AnyFlows 相对单一中转站的真正差异点，
          比再堆一排特性卡片更值得占一屏。
        */}
        <section className="border-t border-[var(--hairline)] bg-[var(--surface-sunken)]">
          <div className="mx-auto max-w-[1120px] px-6 py-32">
            <InView className="mx-auto max-w-2xl text-center">
              <p className="text-xs font-medium tracking-normal text-brand uppercase">
                {t('landing.modes.eyebrow')}
              </p>
              <h2 className="mt-5 text-[clamp(1.875rem,4.5vw,3rem)] leading-[1.1] font-semibold tracking-normal">
                {t('landing.modes.heading')}
              </h2>
              <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-muted-foreground">
                {t('landing.modes.subheading')}
              </p>
            </InView>

            <div className="mt-20 grid gap-4 md:grid-cols-3">
              {deployModes.map(({ key, icon: Icon }, index) => (
                <InView key={key} delay={index * 90}>
                  <SpotlightCard className="h-full rounded-xl border border-[var(--hairline)] bg-[var(--surface-raised)] p-8">
                    {/* 序号用等宽字，与卡内正文形成技术感对照 */}
                    <div className="flex items-center justify-between">
                      <Icon className="size-5 text-brand" aria-hidden="true" />
                      <span className="font-mono text-xs text-muted-foreground/60">
                        {String(index + 1).padStart(2, '0')}
                      </span>
                    </div>
                    <p className="mt-6 text-xs font-medium tracking-normal text-muted-foreground/70 uppercase">
                      {t(`landing.modes.${key}.label`)}
                    </p>
                    <h3 className="mt-2 text-lg font-semibold tracking-normal">
                      {t(`landing.modes.${key}.title`)}
                    </h3>
                    <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
                      {t(`landing.modes.${key}.body`)}
                    </p>
                  </SpotlightCard>
                </InView>
              ))}
            </div>
          </div>
        </section>

        <section className="mx-auto max-w-[1120px] px-6 py-32">
          <InView className="mx-auto max-w-2xl text-center">
            <p className="text-xs font-medium tracking-normal text-brand uppercase">
              {t('landing.protocol.eyebrow')}
            </p>
            <h2 className="mt-5 text-[clamp(1.875rem,4.5vw,3rem)] leading-[1.1] font-semibold tracking-normal">
              {t('landing.protocol.heading')}
            </h2>
            <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-muted-foreground">
              {t('landing.protocol.subheading')}
            </p>
          </InView>

          <InView delay={120}>
            <ProtocolMatrix />
          </InView>
        </section>

        {/* API 调用展示：协议矩阵说明「能做什么」，这里证明「有多简单」 */}
        <section className="border-t border-[var(--hairline)] bg-[var(--surface-sunken)]">
          <div className="mx-auto max-w-[1120px] px-6 py-32">
            <InView className="mx-auto max-w-2xl text-center">
              <p className="text-xs font-medium tracking-normal text-brand uppercase">
                {t('landing.console.eyebrow')}
              </p>
              <h2 className="mt-5 text-[clamp(1.875rem,4.5vw,3rem)] leading-[1.1] font-semibold tracking-normal">
                {t('landing.console.heading')}
              </h2>
              <p className="mx-auto mt-5 max-w-xl text-base leading-relaxed text-muted-foreground">
                {t('landing.console.subheading')}
              </p>
            </InView>

            <InView delay={120}>
              <ApiConsole />
            </InView>
          </div>
        </section>

        {/* 收尾 CTA：与 hero 呼应，但用极光而非粒子，避免两处动效等量齐观 */}
        <section className="relative isolate overflow-hidden border-t border-[var(--hairline)] px-6 py-36">
          <div
            className="pointer-events-none absolute top-1/2 left-1/2 -z-10 h-80 w-[46rem] max-w-[130vw] -translate-x-1/2 -translate-y-1/2 animate-aurora blur-[100px]"
            style={{
              background:
                'radial-gradient(ellipse 60% 50% at 45% 50%, rgb(255 99 99 / 0.14), transparent 70%)',
            }}
            aria-hidden="true"
          />

          <InView className="mx-auto max-w-xl text-center">
            <h2 className="text-[clamp(1.75rem,4vw,2.75rem)] leading-[1.12] font-semibold tracking-normal">
              {t('landing.closing.heading')}
            </h2>
            <p className="mt-5 text-base leading-relaxed text-muted-foreground">
              {t('landing.closing.body')}
            </p>
            <div className="mt-9 flex flex-wrap items-center justify-center gap-2.5">
              <Button size="lg" onClick={openAccount}>
                {t('landing.closing.cta')}
                <ArrowRight className="size-4" aria-hidden="true" />
              </Button>
              <Button variant="secondary" size="lg">
                {t('landing.closing.secondary')}
              </Button>
            </div>
          </InView>
        </section>
      </main>

      <footer className="border-t border-[var(--hairline)] px-6 py-8">
        <div className="mx-auto flex max-w-[1120px] flex-wrap items-center justify-between gap-4 text-sm text-muted-foreground">
          <span>{t('landing.footer.copyright')}</span>
          <span>{t('brand.scope')}</span>
        </div>
      </footer>
    </div>
  )
}
