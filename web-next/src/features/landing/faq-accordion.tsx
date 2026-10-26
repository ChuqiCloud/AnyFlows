import { ChevronDown, HelpCircle } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

interface FaqItem {
  id: string
  question: string
  answer: string
}

export function FaqAccordion() {
  const { t } = useTranslation()
  const [openIds, setOpenIds] = useState<Set<string>>(new Set(['whatIsAnyFlows', 'howRoutingWorks']))

  const toggle = (id: string) => {
    setOpenIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })
  }

  const faqKeys = [
    'whatIsAnyFlows',
    'whichProtocols',
    'howRoutingWorks',
    'howBillingWorks',
    'canSelfHost',
    'whereToStart',
  ]

  const items: FaqItem[] = faqKeys.map((key) => ({
    id: key,
    question: t(`landing.faq.items.${key}.question`),
    answer: t(`landing.faq.items.${key}.answer`),
  }))

  return (
    <section className="border-t border-[var(--hairline)] bg-background py-20 sm:py-24">
      <div className="mx-auto w-full max-w-[960px] px-4 sm:px-6 lg:px-8">
        {/* Header */}
        <div className="text-center">
          <div className="inline-flex items-center gap-1.5 rounded-full border border-primary/20 bg-primary/10 px-3 py-1 text-xs font-medium text-primary">
            <HelpCircle className="size-3.5" />
            <span>{t('landing.faq.eyebrow')}</span>
          </div>
          <h2 className="mt-4 text-3xl font-semibold tracking-[-0.03em] sm:text-4xl text-foreground">
            {t('landing.faq.heading')}
          </h2>
          <p className="mt-4 text-sm leading-6 text-muted-foreground sm:text-base sm:leading-7">
            {t('landing.faq.subheading')}
          </p>
        </div>

        {/* Accordion List */}
        <div className="mt-12 space-y-3">
          {items.map((item) => {
            const isOpen = openIds.has(item.id)

            return (
              <div
                key={item.id}
                className={cn(
                  'rounded-xl border transition-all duration-200 overflow-hidden',
                  isOpen
                    ? 'border-primary/30 bg-surface-1 shadow-sm'
                    : 'border-[var(--hairline)] bg-surface-1/60 hover:border-border hover:bg-surface-1',
                )}
              >
                <button
                  type="button"
                  onClick={() => toggle(item.id)}
                  aria-expanded={isOpen}
                  className="flex w-full items-center justify-between gap-4 p-5 text-left text-base font-medium text-foreground transition-colors cursor-pointer"
                >
                  <span className="font-semibold">{item.question}</span>
                  <ChevronDown
                    className={cn(
                      'size-4.5 shrink-0 text-muted-foreground transition-transform duration-200',
                      isOpen && 'rotate-180 text-primary',
                    )}
                  />
                </button>

                {isOpen && (
                  <div className="border-t border-[var(--hairline)] px-5 pt-3 pb-5 text-sm leading-relaxed text-muted-foreground">
                    <p>{item.answer}</p>
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>
    </section>
  )
}
