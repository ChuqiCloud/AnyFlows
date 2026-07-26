import { cn } from '@/lib/utils'

type ParticleFieldProps = {
  className?: string
  /** 粒子数量，默认 28（再多在低端机上会掉帧且视觉变噪） */
  count?: number
}

/**
 * 确定性伪随机：同一 seed 永远得到同一序列。
 *
 * 刻意不用 Math.random()——粒子位置若在每次渲染时变化，热更新和 StrictMode
 * 的二次渲染都会让整片粒子瞬移。这里要的是「看起来随机」而非真随机。
 */
function noise(seed: number) {
  const value = Math.sin(seed * 12.9898) * 43758.5453

  return value - Math.floor(value)
}

/**
 * 浮尘粒子层：极淡的小光点缓慢上浮，为背景提供细腻的生命感。
 *
 * 所有粒子共用一条 drift 关键帧，差异全部由 CSS 变量承载，因此只有
 * transform / opacity 参与动画，整层跑在合成器上，不触发布局与重绘。
 */
export function ParticleField({ className, count = 28 }: ParticleFieldProps) {
  const particles = Array.from({ length: count }, (_, index) => {
    // 三个互不相关的 seed 偏移，避免位置与时长产生可见的相关性（比如「越靠右越慢」）
    const rx = noise(index + 1)
    const ry = noise(index + 71.3)
    const rt = noise(index + 137.7)

    return {
      key: index,
      left: `${rx * 100}%`,
      top: `${ry * 100}%`,
      // 1~2.6px：超过 3px 就从「浮尘」变成「圆点」了
      size: 1 + rt * 1.6,
      duration: 13 + rt * 15,
      // 负延迟让首帧就处于动画中途，避免整片粒子同时从 opacity:0 启动
      delay: -(rx * 26),
      driftX: `${(rx - 0.5) * 46}px`,
      driftY: `${-(52 + ry * 76)}px`,
      peak: 0.22 + rt * 0.42,
    }
  })

  return (
    <div
      className={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}
      data-particles=""
      aria-hidden="true"
    >
      {particles.map((particle) => (
        <span
          key={particle.key}
          className="absolute animate-drift rounded-full bg-foreground/70 will-change-transform"
          style={
            {
              left: particle.left,
              top: particle.top,
              width: `${particle.size}px`,
              height: `${particle.size}px`,
              animationDuration: `${particle.duration}s`,
              animationDelay: `${particle.delay}s`,
              '--drift-x': particle.driftX,
              '--drift-y': particle.driftY,
              '--drift-peak': particle.peak,
            } as React.CSSProperties
          }
        />
      ))}
    </div>
  )
}
