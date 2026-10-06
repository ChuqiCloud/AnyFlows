import { useEffect, useRef, useState, type ElementType, type ReactNode } from 'react'

import { cn } from '@/lib/utils'

type InViewProps = {
  children: ReactNode
  className?: string
  /** 渲染成什么标签，默认 div。区块用 section、列表项用 li。 */
  as?: ElementType
  /** 入场延迟（毫秒）。同排卡片依次递增即可错开出现。 */
  delay?: number
  /**
   * 元素露出多少比例才触发。默认 0.15——太高会导致长区块滚到底部才入场，
   * 太低则元素刚冒头就播完动画，滚到眼前时已经静止。
   */
  threshold?: number
}

/**
 * 滚动入场容器：元素进入视口时从下方浮起。
 *
 * 动画本身在 CSS 里（[data-inview][data-visible] → fade-up），这里只负责
 * 在恰当时机挂上 data-visible。这样关动效时 CSS 能单方面复位，
 * 不需要 JS 配合——降级路径不依赖脚本执行成功。
 */
export function InView({ children, className, as, delay = 0, threshold = 0.15 }: InViewProps) {
  const Component = as ?? 'div'
  const ref = useRef<HTMLElement>(null)
  /*
   * 初值 true 是刻意的兜底：若 IntersectionObserver 不可用，或 effect 因故
   * 没跑，内容也应当是可见的。宁可不播动画，也不能让内容永久隐身。
   */
  const [visible, setVisible] = useState(true)

  useEffect(() => {
    const el = ref.current

    if (!el || typeof IntersectionObserver === 'undefined') {
      return
    }

    // 关动效时不做观察，保持可见——与 CSS 的降级分支一致
    if (document.documentElement.dataset.motion !== 'full') {
      return
    }

    // 确认可以观察后才转为隐藏，避免"先隐藏再发现观察不了"的窗口期
    setVisible(false)

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) {
            continue
          }

          setVisible(true)
          // 只播一次：来回滚动时反复播放会让人晕
          observer.disconnect()
        }
      },
      // 底部留 -10% 余量，元素真正进入视野中部才触发，而非刚擦到边缘
      { threshold, rootMargin: '0px 0px -10% 0px' },
    )

    observer.observe(el)

    return () => observer.disconnect()
  }, [threshold])

  return (
    <Component
      ref={ref}
      data-inview=""
      data-visible={visible ? '' : undefined}
      className={cn(className)}
      style={delay ? ({ '--inview-delay': `${delay}ms` } as React.CSSProperties) : undefined}
    >
      {children}
    </Component>
  )
}
