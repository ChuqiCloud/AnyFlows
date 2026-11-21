import { providerDisplayName } from '@/components/brand/provider-catalog'
import { Chip } from '@heroui/react'
import { Cpu } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findModelLogo, findProviderLogo } from '@/components/brand/model-logos'
import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import { ModelCopyButton } from './model-copy-button'

/** 模型名仅用于匹配展示图标，厂商归属仍使用模型目录字段。 */
export function ModelCatalogIdentity({ item }: { item: ModelCatalogItem }) {
  const { t } = useTranslation()
  const logo = findProviderLogo(item.provider)
  const ModelIcon = findModelLogo(item.model)?.Icon ?? logo?.Icon ?? Cpu
  return (
    <div className="flex min-w-0 items-start gap-2">
      <div className="relative grid size-8 shrink-0 place-items-center overflow-hidden rounded-md border border-[var(--hairline)] bg-surface-2 text-foreground">
        <ModelIcon className="size-4" aria-hidden="true" />
        {item.icon_url ? (
          <img
            key={item.icon_url}
            src={item.icon_url}
            alt=""
            className="absolute inset-0 size-full object-cover"
            onError={(event) => { event.currentTarget.hidden = true }}
          />
        ) : null}
      </div>
      <div className="min-w-0">
        <div className="truncate text-sm font-semibold text-foreground" title={item.display_name}>{item.display_name}</div>
        <div className="mt-0.5 flex min-w-0 flex-wrap items-center gap-1">
          <span className="max-w-48 truncate font-mono text-[0.6875rem] text-muted-foreground" title={item.model}>{item.model}</span>
          <ModelCopyButton model={item.model} size="icon-xs" />
          <Chip className="max-w-24 truncate bg-info/8 text-info" size="sm" variant="flat">{providerDisplayName(item.provider)}</Chip>
          {item.tags.slice(0, 2).map((tag) => <Chip key={tag} className="max-w-24 truncate" size="sm" variant="flat">{tag}</Chip>)}
          {item.tags.length > 2 ? <Chip size="sm" variant="flat">{t('models.values.moreTags', { count: item.tags.length - 2 })}</Chip> : null}
        </div>
      </div>
    </div>
  )
}
