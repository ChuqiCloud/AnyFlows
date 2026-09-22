import { Button, Skeleton } from '@heroui/react'
import { Boxes, CircleDollarSign, EyeOff, SearchX, Settings2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { ModelCatalogDesktopEntry, ModelCatalogMobileEntry } from './model-catalog-entry'

type ModelCatalogListProps = {
  models: ModelCatalogItem[]
  loading: boolean
  filtered: boolean
  pricingScope?: ModelCatalogPricingScope
  admin: boolean
}

function LoadingRows() {
  return (
    <div className="grid gap-2" role="status" aria-live="polite">
      {[0, 1, 2, 3, 4, 5].map((item) => <Skeleton key={item} className="h-16 rounded-xl" />)}
    </div>
  )
}

function EmptyCatalog({ admin, filtered }: { admin: boolean; filtered: boolean }) {
  const { t } = useTranslation()
  const Icon = filtered ? SearchX : Boxes
  return (
    <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
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
        {!filtered && admin ? (
          <div className="mt-5 text-left">
            <div className="grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
              <PublishingRequirement icon={EyeOff} text={t('models.empty.admin.visibility')} />
              <PublishingRequirement icon={Settings2} text={t('models.empty.admin.lifecycle')} />
              <PublishingRequirement icon={CircleDollarSign} text={t('models.empty.admin.pricing')} />
            </div>
            <Button type="button" size="sm" color="primary" className="mt-3 w-full" as="a" href="/console/system-settings/models">{t('models.empty.admin.action')}</Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}

function PublishingRequirement({ icon: Icon, text }: {
  icon: typeof EyeOff
  text: string
}) {
  return (
    <div className="flex items-start gap-2 text-xs leading-5 text-muted-foreground">
      <Icon className="mt-0.5 size-3.5 shrink-0 text-info" aria-hidden="true" />
      <span>{text}</span>
    </div>
  )
}

export function ModelCatalogList({
  models,
  loading,
  filtered,
  pricingScope,
  admin,
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
  if (models.length === 0) return <EmptyCatalog admin={admin} filtered={filtered} />

  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 xl:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[34%] px-3 py-2 font-medium">{t('models.columns.model')}</th>
              <th className="w-[14%] px-3 py-2 font-medium">{t('models.columns.input')}</th>
              <th className="w-[14%] px-3 py-2 font-medium">{t('models.columns.output')}</th>
              <th className="w-[14%] px-3 py-2 font-medium">{t('models.columns.cacheRead')}</th>
              <th className="w-[24%] px-3 py-2 text-right font-medium">{t('models.columns.status')}</th>
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
      <div className="grid gap-2 xl:hidden">
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
