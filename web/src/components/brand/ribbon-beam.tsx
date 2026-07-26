import { cn } from '@/lib/utils'

type RibbonBeamProps = {
  className?: string
  /** 沿绸带流动的粒子数，默认 18（再多会从「光束」变成「星河」） */
  count?: number
}

/**
 * 绸带的中心线。两条三次贝塞尔拼成一条舒展的 S 形，横贯首屏。
 *
 * 坐标系是 0 0 1200 600 的 viewBox，与下面 SVG 的 viewBox 必须严格一致——
 * 粒子层用 path() 复用这条路径，一旦两边比例不同，粒子就会脱离绸带。
 */
const RIBBON_PATH = 'M -80 392 C 210 300, 330 176, 596 214 C 872 253, 980 148, 1288 96'

/** 与 ParticleField 同一套确定性伪随机，理由见那边的注释 */
function noise(seed: number) {
  const value = Math.sin(seed * 12.9898) * 43758.5453

  return value - Math.floor(value)
}

/**
 * 绸带光束：一条极淡的 S 形光带横贯首屏，附带沿路径流动的细碎粒子。
 *
 * 分三层叠加，每层承担不同的视觉职责：
 *   1. 宽而重模糊的暗色带 —— 只提供体积感，几乎看不见轮廓
 *   2. 窄而亮的芯线       —— 给出「光束」的方向与锐度
 *   3. 流动粒子           —— 让静态的光带产生流向
 *
 * 整层 opacity 压到很低，且不参与命中测试；它是氛围，不该被注意到。
 */
export function RibbonBeam({ className, count = 18 }: RibbonBeamProps) {
  const motes = Array.from({ length: count }, (_, index) => {
    const ra = noise(index + 3.1)
    const rb = noise(index + 53.7)

    return {
      key: index,
      size: 1.4 + rb * 2.2,
      // 18~34s 走完全程：慢到需要盯着才能察觉在动
      duration: 18 + ra * 16,
      // 负延迟铺满整条路径，避免所有粒子挤在起点同时出发
      delay: -(ra * 30),
      peak: 0.3 + rb * 0.45,
    }
  })

  return (
    <div
      className={cn(
        'pointer-events-none absolute inset-0 overflow-hidden [container-type:size]',
        className,
      )}
      data-ribbon=""
      aria-hidden="true"
    >
      <div className="absolute inset-0 animate-ribbon-sway will-change-transform">
        <svg
          className="absolute inset-0 h-full w-full"
          viewBox="0 0 1200 600"
          preserveAspectRatio="xMidYMid slice"
          fill="none"
        >
          <defs>
            {/* 两端透明、中段渐亮：绸带因此没有可见的起点与终点 */}
            <linearGradient id="ribbon-core" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stopColor="var(--ribbon-tint)" stopOpacity="0" />
              <stop offset="26%" stopColor="var(--ribbon-tint)" stopOpacity="0.5" />
              {/* 最亮的中段掺入品牌色：主题切换的色相差异只在这里显形 */}
              <stop offset="52%" stopColor="var(--ribbon-warm)" stopOpacity="0.85" />
              <stop offset="78%" stopColor="var(--ribbon-tint)" stopOpacity="0.45" />
              <stop offset="100%" stopColor="var(--ribbon-tint)" stopOpacity="0" />
            </linearGradient>
            <filter id="ribbon-haze" x="-20%" y="-60%" width="140%" height="220%">
              <feGaussianBlur stdDeviation="26" />
            </filter>
            <filter id="ribbon-glow" x="-10%" y="-40%" width="120%" height="180%">
              <feGaussianBlur stdDeviation="4.5" />
            </filter>
          </defs>

          {/* 体积层：宽 + 重模糊，负责「有东西在那里」 */}
          <path
            d={RIBBON_PATH}
            stroke="url(#ribbon-core)"
            strokeWidth="54"
            strokeLinecap="round"
            filter="url(#ribbon-haze)"
            opacity="0.5"
          />
          {/* 芯线：细 + 轻模糊，负责方向与锐度 */}
          <path
            d={RIBBON_PATH}
            stroke="url(#ribbon-core)"
            strokeWidth="1.6"
            strokeLinecap="round"
            filter="url(#ribbon-glow)"
            opacity="0.9"
          />
        </svg>

        {/*
         * 流动粒子。offset-path 的 path() 与上方 viewBox 共用坐标系，但 CSS 的
         * path() 不随元素尺寸缩放，所以这里保持 1200x600 的原始画布，由
         * [data-ribbon-motes] 的 transform 复刻 SVG 的 slice 缩放与居中。
         */}
        <div
          className="absolute top-0 left-0 h-[600px] w-[1200px] origin-top-left"
          data-ribbon-motes=""
        >
          {motes.map((mote) => (
            <span
              key={mote.key}
              className="absolute animate-ribbon-flow rounded-full bg-[var(--ribbon-accent)] will-change-[offset-distance,opacity]"
              data-ribbon-mote=""
              style={
                {
                  width: `${mote.size}px`,
                  height: `${mote.size}px`,
                  offsetPath: `path('${RIBBON_PATH}')`,
                  animationDuration: `${mote.duration}s`,
                  animationDelay: `${mote.delay}s`,
                  '--flow-peak': mote.peak,
                } as React.CSSProperties
              }
            />
          ))}
        </div>
      </div>
    </div>
  )
}
