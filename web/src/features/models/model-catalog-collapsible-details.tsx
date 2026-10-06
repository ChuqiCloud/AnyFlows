import { useEffect, useState, type ReactNode } from 'react'

import type { ModelCatalogItem, ModelCatalogPricingScope } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { ModelCatalogExpandedDetails } from './model-catalog-expanded-details'

type DetailsProps = {
  expanded: boolean
  id: string
  item: ModelCatalogItem
  pricingScope: ModelCatalogPricingScope
}

const COLLAPSE_DURATION_MS = 320

/** 保留退出阶段的 DOM，使详情在快速反向操作时也能连续展开或收起。 */
function useCollapsiblePresence(expanded: boolean) {
  const [present, setPresent] = useState(expanded)
  const [visible, setVisible] = useState(expanded)

  useEffect(() => {
    let frame: number | undefined
    let timeout: ReturnType<typeof setTimeout> | undefined

    if (expanded) {
      setPresent(true)
      frame = window.requestAnimationFrame(() => setVisible(true))
    } else {
      setVisible(false)
      timeout = setTimeout(() => setPresent(false), COLLAPSE_DURATION_MS)
    }

    return () => {
      if (frame !== undefined) window.cancelAnimationFrame(frame)
      if (timeout !== undefined) clearTimeout(timeout)
    }
  }, [expanded])

  return { present, visible }
}

function AnimatedDetailsContent({
  children,
  id,
  visible,
}: {
  children: ReactNode
  id: string
  visible: boolean
}) {
  return (
    <div
      id={id}
      role="region"
      aria-hidden={!visible}
      data-model-details-motion
      className={cn(
        'grid overflow-hidden transition-[grid-template-rows,opacity] duration-[320ms] ease-[cubic-bezier(0.16,1,0.3,1)]',
        visible ? 'grid-rows-[1fr] opacity-100' : 'pointer-events-none grid-rows-[0fr] opacity-0',
      )}
    >
      <div className="min-h-0 overflow-hidden">
        <div
          data-model-details-content
          className={cn(
            'transition-transform duration-300 ease-[cubic-bezier(0.16,1,0.3,1)]',
            visible ? 'translate-y-0' : '-translate-y-1',
          )}
        >
          {children}
        </div>
      </div>
    </div>
  )
}

/** 桌面表格详情行在关闭动画结束后再卸载，避免行高瞬间跳变。 */
export function ModelCatalogDesktopDetailsRow({ expanded, id, item, pricingScope }: DetailsProps) {
  const { present, visible } = useCollapsiblePresence(expanded)
  if (!present) return null

  return (
    <tr className="bg-surface-2/20" aria-hidden={!visible}>
      <td colSpan={5} className="p-0">
        <AnimatedDetailsContent id={id} visible={visible}>
          <div className="px-3 py-3">
            <ModelCatalogExpandedDetails item={item} pricingScope={pricingScope} />
          </div>
        </AnimatedDetailsContent>
      </td>
    </tr>
  )
}

/** 移动卡片沿用相同节奏，并把分隔线包含在裁剪区域内。 */
export function ModelCatalogMobileDetails({ expanded, id, item, pricingScope }: DetailsProps) {
  const { present, visible } = useCollapsiblePresence(expanded)
  if (!present) return null

  return (
    <AnimatedDetailsContent id={id} visible={visible}>
      <div className="mt-3 border-t border-[var(--hairline)] pt-3">
        <ModelCatalogExpandedDetails item={item} pricingScope={pricingScope} />
      </div>
    </AnimatedDetailsContent>
  )
}
