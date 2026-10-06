import type { ComponentProps } from 'react'
import { cva, type VariantProps } from 'class-variance-authority'

import { cn } from '@/lib/utils'

// 锐利极简：容器靠 1px 细边界定，不用阴影堆叠质感。
const cardVariants = cva('rounded-xl border border-[var(--hairline)] bg-card text-card-foreground', {
  variants: {
    elevation: {
      flat: '',
      /** 浮层：弹窗、命令面板、下拉——唯一允许用阴影的场景 */
      overlay: 'shadow-overlay',
    },
    interactive: {
      true: 'transition-[border-color,background-color] duration-200 hover:border-white/16 hover:bg-surface-2/40 light:hover:border-black/16',
      false: '',
    },
  },
  defaultVariants: {
    elevation: 'flat',
    interactive: false,
  },
})

type CardProps = ComponentProps<'div'> & VariantProps<typeof cardVariants>

export function Card({ className, elevation, interactive, ...props }: CardProps) {
  return <div data-slot="card" className={cn(cardVariants({ elevation, interactive, className }))} {...props} />
}

export function CardHeader({ className, ...props }: ComponentProps<'div'>) {
  return <div data-slot="card-header" className={cn('flex flex-col gap-1.5 p-5', className)} {...props} />
}

export function CardTitle({ className, ...props }: ComponentProps<'h3'>) {
  return <h3 data-slot="card-title" className={cn('text-[0.9375rem] leading-none font-semibold', className)} {...props} />
}

export function CardDescription({ className, ...props }: ComponentProps<'p'>) {
  return <p data-slot="card-description" className={cn('text-sm text-muted-foreground', className)} {...props} />
}

export function CardContent({ className, ...props }: ComponentProps<'div'>) {
  return <div data-slot="card-content" className={cn('p-5 pt-0', className)} {...props} />
}

export { cardVariants }
