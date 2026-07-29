import { Boxes, SearchX } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Skeleton } from '@/components/ui/skeleton'
import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { ModelCatalogDesktopEntry, ModelCatalogMobileEntry } from './model-catalog-entry'

type ModelCatalogListProps = {
  models: ModelCatalogItem[]
  loading: boolean
  filtered: boolean
  pricingScope?: ModelCatalogPricingScope
}

function LoadingRows() {
  return (
    <div className="grid gap-2" role="status" aria-live="polite">
      {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
    </div>
  )
}

function EmptyCatalog({ filtered }: { filtered: boolean }) {
  const { t } = useTranslation()
  const Icon = filtered ? SearchX : Boxes
  return (
    <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
      <div className="max-w-xs">
        <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
          <Icon className="size-4" aria-hidden="true" />
        </div>
        <h2 className="mt-3 text-sm font-semibold">
          {t(filtered ? 'models.empty.filteredTitle' : 'models.empty.title')}
        </h2>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {t(filtered ? 'models.empty.filteredBody' : 'models.empty.body')}
        </p>
      </div>
    </div>
  )
}

export function ModelCatalogList({
  models,
  loading,
  filtered,
  pricingScope,
}: ModelCatalogListProps) {
  const { t } = useTranslation()
  const [expandedModels, setExpandedModels] = useState<Set<string>>(() => new Set())
  const toggle = (model: string) => {
    setExpandedModels((current) => {
      const next = new Set(current)
      if (next.has(model)) next.delete(model)
      else next.add(model)
      return next
    })
  }

  if (loading || !pricingScope) return <LoadingRows />
  if (models.length === 0) return <EmptyCatalog filtered={filtered} />

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[31%] px-3 py-2 font-medium">{t('models.columns.model')}</th>
              <th className="w-[15%] px-3 py-2 font-medium">{t('models.columns.input')}</th>
              <th className="w-[15%] px-3 py-2 font-medium">{t('models.columns.output')}</th>
              <th className="w-[17%] px-3 py-2 font-medium">{t('models.columns.cacheRead')}</th>
              <th className="w-[22%] px-3 py-2 text-right font-medium">{t('models.columns.billing')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {models.map((item) => (
              <ModelCatalogDesktopEntry
                key={item.model}
                item={item}
                pricingScope={pricingScope}
                expanded={expandedModels.has(item.model)}
                onToggle={() => toggle(item.model)}
              />
            ))}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:hidden">
        {models.map((item) => (
          <ModelCatalogMobileEntry
            key={item.model}
            item={item}
            pricingScope={pricingScope}
            expanded={expandedModels.has(item.model)}
            onToggle={() => toggle(item.model)}
          />
        ))}
      </div>
    </>
  )
}
