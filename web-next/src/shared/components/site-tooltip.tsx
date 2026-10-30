import { Tooltip } from '@heroui/react'
import type { TooltipProps } from '@heroui/react'
import { cloneElement, useCallback, useRef, useState, type ReactElement, type Ref } from 'react'

import { cn } from '@/lib/utils'

type TriggerRef = Ref<HTMLElement>

type SiteTooltipProps = Omit<TooltipProps, 'children' | 'isOpen' | 'onOpenChange'> & {
  children: ReactElement<{ ref?: TriggerRef }>
  /** 折叠侧栏等场景下关掉背景与边框，让 Tooltip 透明地叠在原位置旁。 */
  bare?: boolean
}

/**
 * HeroUI 的 Tooltip 靠 cloneElement 把 onHoverStart/onHoverEnd 注入触发元素，
 * 但 HeroUI 的 Button 会用 filterDOMProps 过滤入参（只放行 id 与 data-*），
 * 这两个 React Aria 专有事件名必然被丢掉，于是只有聚焦能弹出、鼠标悬停没反应。
 * 这里改为直接监听触发元素的原生鼠标与焦点事件来驱动 Tooltip，不依赖被包装组件是否透传。
 *
 * 折叠侧栏等只需要"读出文字"的场景传 `bare`，剥掉默认的实心背景/边框/箭头。
 */
export function SiteTooltip({ children, classNames, bare = false, ...tooltipProps }: SiteTooltipProps) {
  const [isOpen, setIsOpen] = useState(false)
  const detach = useRef<(() => void) | null>(null)
  const childRef = children.props.ref

  const setTrigger = useCallback(
    (node: HTMLElement | null) => {
      detach.current?.()
      detach.current = null
      assignRef(childRef, node)
      if (!node) return

      const listeners: Array<[string, (event: Event) => void]> = [
        ['mouseenter', () => setIsOpen(true)],
        ['mouseleave', () => setIsOpen(false)],
        ['focusin', () => setIsOpen(true)],
        ['focusout', () => setIsOpen(false)],
      ]

      listeners.forEach(([name, listener]) => node.addEventListener(name, listener))
      detach.current = () => listeners.forEach(([name, listener]) => node.removeEventListener(name, listener))
    },
    [childRef],
  )

  return (
    <Tooltip
      classNames={bare ? {
        ...classNames,
        // HeroUI 的 popover 把背景写在 content slot 的 default variant 里（bg-content1 + shadow-small），
        // tailwind-merge 会把我们传入的 bg-transparent 当作同名组冲突而丢弃，必须加 ! 强制覆盖。
        base: cn('!bg-transparent !shadow-none !border-0 !p-0', classNames?.base),
        content: cn(
          '!bg-transparent !shadow-none !border-0 !px-2 !py-1 text-xs font-medium text-foreground',
          classNames?.content,
        ),
        arrow: cn('!hidden', classNames?.arrow),
      } : classNames}
      isOpen={isOpen}
      onOpenChange={setIsOpen}
      {...tooltipProps}
      // Tooltip 自带的箭头在 bare 模式下也没必要出现。
      {...(bare ? { showArrow: false } : {})}
    >
      {cloneElement(children, { ref: setTrigger })}
    </Tooltip>
  )
}

/** 合并调用方自带的 ref，避免覆盖。 */
function assignRef<T>(ref: Ref<T> | undefined, value: T | null) {
  if (typeof ref === 'function') ref(value)
  else if (ref) (ref as { current: T | null }).current = value
}
