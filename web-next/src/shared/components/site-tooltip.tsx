import { Tooltip } from '@heroui/react'
import type { TooltipProps } from '@heroui/react'
import { cloneElement, useCallback, useRef, useState, type ReactElement, type Ref } from 'react'

type TriggerRef = Ref<HTMLElement>

type SiteTooltipProps = Omit<TooltipProps, 'children' | 'isOpen' | 'onOpenChange'> & {
  children: ReactElement<{ ref?: TriggerRef }>
}

/**
 * HeroUI 的 Tooltip 靠 cloneElement 把 onHoverStart/onHoverEnd 注入触发元素，
 * 但 HeroUI 的 Button 会用 filterDOMProps 过滤入参（只放行 id 与 data-*），
 * 这两个 React Aria 专有事件名必然被丢掉，于是只有聚焦能弹出、鼠标悬停没反应。
 * 这里改为直接监听触发元素的原生鼠标与焦点事件来驱动 Tooltip，不依赖被包装组件是否透传。
 */
export function SiteTooltip({ children, ...tooltipProps }: SiteTooltipProps) {
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
    <Tooltip isOpen={isOpen} onOpenChange={setIsOpen} {...tooltipProps}>
      {cloneElement(children, { ref: setTrigger })}
    </Tooltip>
  )
}

/** 合并调用方自带的 ref，避免覆盖。 */
function assignRef<T>(ref: Ref<T> | undefined, value: T | null) {
  if (typeof ref === 'function') ref(value)
  else if (ref) (ref as { current: T | null }).current = value
}
