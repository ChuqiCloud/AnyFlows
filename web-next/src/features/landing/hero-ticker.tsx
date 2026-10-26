import { Cpu } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findProviderLogo } from '@/components/brand/model-logos'
import { providerDisplayName } from '@/components/brand/provider-catalog'
import { useLandingModels } from './use-landing-models'

export function HeroTicker() {
  const { t } = useTranslation()
  const { models } = useLandingModels()
  const items = models.slice(0, 8)
  const duplicated = [...items, ...items]
  if (items.length === 0) return null

  return (
    <div
      className="relative w-full border-y border-[var(--hairline)] bg-surface-1/75 backdrop-blur-md overflow-hidden h-10.5 flex items-center"
      aria-label={t('landing.catalog.heading')}
    >
      <div className="absolute left-0 top-0 bottom-0 w-12 z-10 bg-gradient-to-r from-background to-transparent pointer-events-none" />
      <div className="absolute right-0 top-0 bottom-0 w-12 z-10 bg-gradient-to-l from-background to-transparent pointer-events-none" />

      <div
        className="group/ticker flex w-max shrink-0 animate-marquee items-center hover:[animation-play-state:paused] motion-reduce:animate-none"
        style={{ animationDuration: '42s' }}
      >
        {duplicated.map((item, index) => {
          const Icon = findProviderLogo(item.provider)?.Icon ?? Cpu

          return (
            <div
              key={`${item.model}-${index}`}
              aria-hidden={index >= items.length}
              className="inline-flex items-center gap-2.5 px-6 border-r border-[var(--hairline)] text-xs text-muted-foreground whitespace-nowrap cursor-default"
            >
              <Icon className="size-3.5 text-foreground/80 shrink-0" />
              <span className="font-medium text-foreground/90">{item.display_name}</span>
              <span className="inline-flex items-center gap-1 font-mono text-[0.6875rem] text-muted-foreground">
                {providerDisplayName(item.provider)}
              </span>
              <span className="font-mono text-[0.625rem] text-muted-foreground/70">
                {t(`models.billing.${item.billing_mode}`)}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
