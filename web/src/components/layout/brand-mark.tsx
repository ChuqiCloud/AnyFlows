import { cn } from '@/lib/utils'

type BrandMarkProps = {
  className?: string
  size?: 'nav' | 'hero'
}

/**
 * AnyFlows 标识：三道渐次收束的斜线表达「多路汇流」。
 * 纯 CSS 生成，无图片资源，扁平无浮雕。
 */
export function BrandMark({ className, size = 'nav' }: BrandMarkProps) {
  return (
    <div
      className={cn(
        'relative grid shrink-0 place-items-center overflow-hidden bg-brand',
        size === 'nav' ? 'size-8 rounded-lg' : 'size-14 rounded-2xl',
        className,
      )}
      aria-hidden="true"
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        className={size === 'nav' ? 'size-4.5' : 'size-8'}
        stroke="var(--brand-foreground)"
        strokeWidth="2.5"
        strokeLinecap="round"
      >
        <path d="M3 6h18" opacity="0.55" />
        <path d="M6 12h12" opacity="0.8" />
        <path d="M9 18h6" />
      </svg>
    </div>
  )
}
