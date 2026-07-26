import type { ComponentProps } from 'react'
import { useRef } from 'react'

import { cn } from '@/lib/utils'

/**
 * 指针跟随微光卡片。
 *
 * 与首页的 LogoField 同一套语言：光跟着指针走，只是尺度更小。
 * 坐标写进 CSS 变量，不触发 React 重渲染。
 */
export function SpotlightCard({ className, children, ...props }: ComponentProps<'div'>) {
  const ref = useRef<HTMLDivElement>(null)

  const handleMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const el = ref.current

    if (!el) {
      return
    }

    const rect = el.getBoundingClientRect()
    el.style.setProperty('--card-x', `${event.clientX - rect.left}px`)
    el.style.setProperty('--card-y', `${event.clientY - rect.top}px`)
  }

  return (
    <div
      ref={ref}
      onPointerMove={handleMove}
      className={cn(
        'group/spot relative isolate overflow-hidden bg-background transition-colors duration-300',
        className,
      )}
      {...props}
    >
      {/* 微光层：默认透明，悬停时以指针为中心晕开 */}
      <div
        className="pointer-events-none absolute inset-0 -z-10 opacity-0 transition-opacity duration-300 group-hover/spot:opacity-100"
        style={{
          background:
            'radial-gradient(340px circle at var(--card-x, 50%) var(--card-y, 50%), rgb(255 99 99 / 0.07), transparent 62%)',
        }}
        aria-hidden="true"
      />
      {children}
    </div>
  )
}
