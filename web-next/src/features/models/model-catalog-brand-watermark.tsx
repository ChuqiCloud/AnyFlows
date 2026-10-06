import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import { findProviderLogo } from '@/components/brand/model-logos'
import { cn } from '@/lib/utils'

type ModelCatalogBrandWatermarkProps = {
  className?: string
  expanded: boolean
  item: ModelCatalogItem
  markClassName?: string
}

/**
 * 在模型条目右侧呈现低对比度厂商标识，并仅在交互状态下增强动势。
 * 水印不参与信息表达，未知厂商且没有自定义图标时直接省略。
 */
export function ModelCatalogBrandWatermark({
  className,
  expanded,
  item,
  markClassName,
}: ModelCatalogBrandWatermarkProps) {
  const logo = findProviderLogo(item.provider)
  const ProviderIcon = logo?.Icon

  if (!ProviderIcon && !item.icon_url) return null

  return (
    <div
      aria-hidden="true"
      className={cn('model-catalog-watermark pointer-events-none absolute z-0 overflow-hidden', className)}
    >
      <div
        data-model-watermark-mark
        className={cn(
          'absolute right-1 top-1/2 size-24 -translate-y-1/2 -rotate-6 scale-[0.92] text-foreground opacity-[0.045]',
          'transition-[transform,opacity] duration-500 ease-[cubic-bezier(0.16,1,0.3,1)]',
          'group-hover:rotate-0 group-hover:scale-[1.04] group-hover:opacity-[0.075]',
          'group-focus-within:rotate-0 group-focus-within:scale-[1.04] group-focus-within:opacity-[0.075]',
          'dark:opacity-[0.065] dark:group-hover:opacity-[0.095] dark:group-focus-within:opacity-[0.095]',
          expanded && 'rotate-[5deg] scale-110 opacity-[0.09] dark:opacity-[0.12]',
          markClassName,
        )}
      >
        <div className={cn('relative size-full', expanded && 'model-catalog-logo-breathe')}>
          {ProviderIcon ? <ProviderIcon className="size-full" /> : null}
          {item.icon_url ? (
            <img
              key={item.icon_url}
              src={item.icon_url}
              alt=""
              className="absolute inset-0 size-full object-contain grayscale"
              onError={(event) => { event.currentTarget.hidden = true }}
            />
          ) : null}
        </div>
      </div>
    </div>
  )
}
