import { Boxes, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findProviderLogo } from '@/components/brand/model-logos'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import type { ModelCatalogProvider } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { toggleModelCatalogFilter } from './model-catalog-filter-model'

type ModelCatalogProviderFilterProps = {
  providers: ModelCatalogProvider[]
  selected: string[]
  loading: boolean
  error: boolean
  onChange: (providers: string[]) => void
  onRetry: () => void
}

/** 展示当前目录完整供应商聚合，不从已加载的模型分页临时推断选项。 */
export function ModelCatalogProviderFilter(props: ModelCatalogProviderFilterProps) {
  const { t } = useTranslation()
  const total = props.providers.reduce((sum, provider) => sum + provider.model_count, 0)

  return (
    <section className="border-t border-[var(--hairline)] py-4">
      <div className="mb-2.5 flex items-center justify-between gap-3">
        <h3 className="text-xs font-semibold">{t('models.filters.providers')}</h3>
        {props.selected.length > 0 ? (
          <span className="text-[0.6875rem] text-muted-foreground tabular-nums">
            {t('models.filters.selectedCount', { count: props.selected.length })}
          </span>
        ) : null}
      </div>

      {props.loading ? (
        <div className="grid gap-1.5" aria-label={t('models.filters.providersLoading')}>
          {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-9 rounded-lg" />)}
        </div>
      ) : props.error ? (
        <div className="rounded-lg border border-destructive/20 bg-destructive/8 p-3">
          <p className="text-xs leading-5 text-muted-foreground">{t('models.filters.providersError')}</p>
          <Button type="button" size="xs" variant="ghost" className="mt-1.5" onClick={props.onRetry}>
            <RefreshCw aria-hidden="true" />
            {t('models.filters.retryProviders')}
          </Button>
        </div>
      ) : (
        <div className="grid gap-1">
          <ProviderOption
            checked={props.selected.length === 0}
            count={total}
            label={t('models.filters.allProviders')}
            onCheckedChange={(checked) => checked && props.onChange([])}
          />
          {props.providers.map((provider) => (
            <ProviderOption
              key={provider.name}
              provider={provider.name}
              checked={props.selected.includes(provider.name)}
              count={provider.model_count}
              label={findProviderLogo(provider.name)?.name ?? provider.name}
              onCheckedChange={(checked) => props.onChange(toggleModelCatalogFilter(
                props.selected,
                provider.name,
                checked,
              ))}
            />
          ))}
        </div>
      )}
    </section>
  )
}

function ProviderOption(props: {
  checked: boolean
  count: number
  label: string
  provider?: string
  onCheckedChange: (checked: boolean) => void
}) {
  const logo = props.provider ? findProviderLogo(props.provider) : undefined
  const ProviderIcon = logo?.Icon ?? Boxes
  return (
    <label className={cn(
      'flex min-h-9 cursor-pointer items-center gap-2 rounded-lg px-2 text-xs transition-colors',
      props.checked
        ? 'bg-primary/10 text-foreground'
        : 'text-muted-foreground hover:bg-surface-2/70 hover:text-foreground',
    )}>
      <Checkbox
        checked={props.checked}
        onCheckedChange={(value) => props.onCheckedChange(value === true)}
      />
      <span className="grid size-6 shrink-0 place-items-center rounded-md bg-surface-2 text-foreground">
        <ProviderIcon className="size-3.5" aria-hidden="true" />
      </span>
      <span className="min-w-0 flex-1 truncate">{props.label}</span>
      <span className="min-w-6 text-right text-[0.6875rem] text-muted-foreground tabular-nums">
        {props.count}
      </span>
    </label>
  )
}
