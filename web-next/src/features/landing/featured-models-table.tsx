import { ArrowRight, Check, Copy, ExternalLink, Sparkles } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink } from 'react-router-dom'

import { ModelCatalogIdentity } from '@/features/models/model-catalog-identity'
import { formatContextWindow } from '@/features/models/model-context-window'
import { ModelPriceValue } from '@/features/models/model-price-value'
import { copyText } from '@/lib/clipboard'
import { useLandingModels } from './use-landing-models'

export function FeaturedModelsTable() {
  const { t } = useTranslation()
  const { query, models } = useLandingModels()
  const featuredModels = models.slice(0, 5)
  const pricingScope = query.data?.pages[0]?.pricing_scope
  const [copiedId, setCopiedId] = useState<string | null>(null)

  const handleCopy = async (id: string) => {
    const success = await copyText(id)
    if (success) {
      setCopiedId(id)
      setTimeout(() => setCopiedId(null), 2000)
    }
  }

  return (
    <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      {/* Section Header */}
      <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-6 border-b border-[var(--hairline)] pb-8">
        <div>
          <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
            <Sparkles className="size-3.5" />
            <span>{t('landing.catalog.eyebrow')}</span>
          </div>
          <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
            {t('landing.catalog.heading')}
          </h2>
          <p className="mt-2.5 max-w-2xl text-sm leading-6 text-muted-foreground sm:text-base">
            {t(pricingScope ? `models.scope.${pricingScope}.subtitle` : 'models.subtitle')}
          </p>
        </div>

        <RouterLink
          to="/models"
          className="inline-flex items-center gap-2 text-sm font-semibold text-primary hover:text-primary/80 transition-colors shrink-0 group"
        >
          <span>{t('landing.catalog.viewAll')}</span>
          <ArrowRight className="size-4 transition-transform group-hover:translate-x-1" />
        </RouterLink>
      </div>

      {/* Pricing Matrix Table */}
      <div className="mt-8 overflow-x-auto rounded-2xl border border-[var(--hairline)] bg-surface-1/70 shadow-xs backdrop-blur-sm">
        <table className="w-full min-w-[760px] text-left border-collapse">
          <thead>
            <tr className="border-b border-[var(--hairline)] bg-surface-sunken/60 text-[0.6875rem] font-mono uppercase tracking-[0.12em] text-muted-foreground">
              <th className="py-3.5 px-6 font-semibold">{t('models.title')}</th>
              <th className="py-3.5 px-5 font-semibold">{t('landing.catalog.context')}</th>
              <th className="py-3.5 px-5 font-semibold">{t('models.columns.input')}</th>
              <th className="py-3.5 px-5 font-semibold">{t('models.columns.output')}</th>
              <th className="py-3.5 px-5 font-semibold">{t('landing.catalog.billing')}</th>
              <th className="py-3.5 px-6 font-semibold text-right">{t('landing.catalog.modelId')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)] text-sm">
            {featuredModels.map((item) => {
              const isCopied = copiedId === item.model

              return (
                <tr
                  key={item.model}
                  className="transition-colors hover:bg-surface-2/60 group"
                >
                  <td className="py-4 px-6"><ModelCatalogIdentity item={item} /></td>
                  <td className="py-4 px-5 font-mono text-xs text-muted-foreground">
                    {item.context_window == null ? t('models.values.contextUnknownShort') : formatContextWindow(item.context_window)}
                  </td>
                  <td className="py-4 px-5">{item.billing_mode === 'free' ? t('models.billing.free') : <ModelPriceValue label={t('models.columns.input')} value={item.prices?.input} />}</td>
                  <td className="py-4 px-5">{item.billing_mode === 'free' ? t('models.billing.free') : <ModelPriceValue label={t('models.columns.output')} value={item.prices?.output} />}</td>
                  <td className="py-4 px-5"><span className="rounded-full border border-primary/25 bg-primary/10 px-2.5 py-0.5 text-xs font-medium text-primary">{t(`models.billing.${item.billing_mode}`)}</span></td>

                  {/* Copy Action */}
                  <td className="py-4 px-6 text-right">
                    <button
                      type="button"
                      onClick={() => handleCopy(item.model)}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-surface-2/80 px-2.5 py-1 text-xs font-mono text-muted-foreground transition-all hover:border-primary/40 hover:bg-surface-2 hover:text-foreground active:scale-95 cursor-pointer"
                      aria-label={t('landing.catalog.copy', { model: item.model })}
                    >
                      {isCopied ? (
                        <>
                          <Check className="size-3 text-success" />
                          <span className="text-success font-medium">{t('landing.home.terminal.copied')}</span>
                        </>
                      ) : (
                        <>
                          <Copy className="size-3" />
                          <span>{item.model}</span>
                        </>
                      )}
                    </button>
                  </td>
                </tr>
              )
            })}
            {models.length === 0 ? <tr><td colSpan={6} className="px-6 py-10 text-center text-sm text-muted-foreground" role={query.isError ? 'alert' : 'status'}>
              {t(query.isPending ? 'models.loading' : query.isError ? 'models.error.body' : 'landing.catalog.empty')}
              {query.isError ? <button type="button" className="ml-2 text-primary underline" onClick={() => void query.refetch()}>{t('models.actions.retry')}</button> : null}
            </td></tr> : null}
          </tbody>
        </table>
      </div>

      {/* Micro-notes */}
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground px-1">
        <span>{t('landing.catalog.note')}</span>
        <RouterLink to="/models" className="hover:text-foreground inline-flex items-center gap-1 font-medium">
          {t('landing.catalog.viewAll')} <ExternalLink className="size-3" />
        </RouterLink>
      </div>
    </section>
  )
}
