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
        'group/spot relative isolate overflow-hidden bg-surface-1 shadow-[var(--shadow-sm)] transition-[transform,box-shadow,border-color,background-color] duration-[350ms] ease-[var(--ease-ai-out)] hover:-translate-y-1 hover:shadow-[var(--shadow-md)] motion-reduce:transform-none motion-reduce:transition-none',
        className,
      )}
      {...props}
    >
      {/* 固定柔光用于区分白色卡片与页面底色，位置统一在右下角，避免干扰正文。 */}
      <div
        className="pointer-events-none absolute inset-0 -z-10"
        style={{
          background:
            'radial-gradient(58% 52% at 88% 108%, color-mix(in oklab, var(--info) 16%, transparent), transparent 72%)',
        }}
        aria-hidden="true"
      />

      {/* 指针微光只在悬停时出现，用较小范围反馈当前交互位置。 */}
      <div
        className="pointer-events-none absolute inset-0 -z-10 opacity-0 transition-opacity duration-300 group-hover/spot:opacity-100"
        style={{
          background:
            'radial-gradient(340px circle at var(--card-x, 50%) var(--card-y, 50%), rgb(91 95 246 / 0.11), transparent 62%)',
        }}
        aria-hidden="true"
      />
      {children}
    </div>
  )
}
