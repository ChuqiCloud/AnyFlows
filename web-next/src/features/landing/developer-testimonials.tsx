import { MessageSquareQuote, Code2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

interface TestimonialItem {
  quote: string
  author: string
  role: string
  avatar: string
}

export function DeveloperTestimonials() {
  const { t } = useTranslation()
  const rawItems = t('landing.testimonials.items', { returnObjects: true })
  const items: TestimonialItem[] = Array.isArray(rawItems) ? (rawItems as TestimonialItem[]) : []

  return (
    <section className="border-t border-[var(--hairline)] bg-surface-sunken/40 py-20 sm:py-24">
      <div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8">
        {/* Header */}
        <div className="mx-auto max-w-2xl text-center">
          <div className="inline-flex items-center gap-1.5 rounded-full border border-primary/20 bg-primary/10 px-3 py-1 text-xs font-medium text-primary">
            <MessageSquareQuote className="size-3.5" />
            <span>{t('landing.testimonials.eyebrow')}</span>
          </div>
          <h2 className="mt-4 text-3xl font-semibold tracking-[-0.03em] sm:text-4xl text-foreground">
            {t('landing.testimonials.heading')}
          </h2>
          <p className="mt-4 text-sm leading-6 text-muted-foreground sm:text-base sm:leading-7">
            {t('landing.testimonials.subheading')}
          </p>
        </div>

        {/* Testimonials Grid */}
        <div className="mt-14 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {items.map((item, index) => (
            <article
              key={`${item.author}-${index}`}
              className="group relative flex flex-col justify-between rounded-xl border border-[var(--hairline)] bg-surface-1 p-5 shadow-xs transition-all duration-300 hover:-translate-y-1 hover:border-primary/30 hover:shadow-md"
            >
              <div className="flex items-center gap-2 text-primary"><Code2 className="size-4" aria-hidden="true" /><span className="font-mono text-xs">{item.role}</span></div>

              {/* Quote */}
              <p className="mt-4 text-sm leading-relaxed text-foreground/90 font-normal">
                {item.quote}
              </p>

              {/* Author Footer */}
              <div className="mt-6 flex items-center gap-3 border-t border-[var(--hairline)] pt-4">
                <div className="size-8 rounded-full bg-gradient-to-tr from-primary/30 via-brand/20 to-success/30 flex items-center justify-center text-xs font-bold text-foreground ring-1 ring-[var(--hairline)]">
                  {item.avatar}
                </div>
                <div>
                  <div className="text-xs font-semibold text-foreground">
                    {item.author}
                  </div>
                  <div className="text-[0.6875rem] text-muted-foreground">
                    {item.role}
                  </div>
                </div>
              </div>
            </article>
          ))}
        </div>
      </div>
    </section>
  )
}
