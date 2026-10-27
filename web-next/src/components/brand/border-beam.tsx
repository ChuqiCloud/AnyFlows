import { cn } from '@/lib/utils'

type BorderBeamProps = {
  className?: string
  /** 边框光束在整个周长中的占比（默认 28%） */
  beamSize?: number
  /** 边框圆角（像素或与父元素一致） */
  radius?: number
  /** 边框粗细 */
  borderWidth?: number
}

/**
 * 边框流动光束。
 * 使用 SVG pathLength 归一化特性，让光束自适应任意宽高比例的容器，无需额外监听尺寸。
 */
export function BorderBeam({
  className,
  beamSize = 28,
  radius = 16,
  borderWidth = 1.5,
}: BorderBeamProps) {
  const gap = 100 - beamSize

  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-0 z-10 overflow-hidden rounded-[inherit]',
        className,
      )}
      aria-hidden="true"
    >
      <svg
        className="size-full overflow-visible"
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
      >
        <defs>
          <linearGradient id="border-beam-line" x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.1" />
            <stop offset="50%" stopColor="var(--primary)" stopOpacity="0.95" />
            <stop offset="85%" stopColor="var(--brand)" stopOpacity="0.8" />
            <stop offset="100%" stopColor="transparent" stopOpacity="0" />
          </linearGradient>
        </defs>
        <rect
          x={borderWidth / 2}
          y={borderWidth / 2}
          width={`calc(100% - ${borderWidth}px)`}
          height={`calc(100% - ${borderWidth}px)`}
          rx={radius}
          pathLength={100}
          stroke="url(#border-beam-line)"
          strokeWidth={borderWidth}
          strokeDasharray={`${beamSize} ${gap}`}
          className="animate-border-beam"
        />
      </svg>
    </div>
  )
}
