import { modelLogos } from '@/components/brand/model-logos'

/** 取前若干个作为路由目标示意，顺序固定以保证视觉稳定 */
const targets = modelLogos.slice(0, 4)

/**
 * 智能路由可视化：单一入口向多个上游渠道扇出，光点沿连线循环流动。
 *
 * 纯 SVG + CSS 动画，不引图表库——这里要的是「示意」而非真实数据，
 * 一张会呼吸的示意图比一个静态图标更能说明路由在做什么。
 */
export function RouteVisual() {
  return (
    <div className="relative mt-8 h-44 w-full overflow-hidden rounded-lg border border-[var(--hairline)] bg-[var(--surface-sunken)]">
      <svg
        className="absolute inset-0 h-full w-full"
        viewBox="0 0 320 176"
        fill="none"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        {targets.map((_, index) => {
          // 四条贝塞尔从左侧同一点出发，落到右侧四个等距端点
          const endY = 32 + index * 37
          const path = `M 44 88 C 140 88, 170 ${endY}, 268 ${endY}`

          return (
            <g key={index}>
              <path d={path} stroke="var(--hairline-strong)" strokeWidth="1" />
              <circle r="2.5" fill="var(--brand)" data-flow-dot="">
                {/* offset-path 在 Safari 支持不稳，这里用 SMIL 沿同一条 path 走 */}
                <animateMotion dur="3.2s" repeatCount="indefinite" begin={`${index * 0.8}s`} path={path} />
              </circle>
            </g>
          )
        })}
      </svg>

      {/* 入口节点 */}
      <div className="absolute top-1/2 left-6 -translate-y-1/2">
        <div className="grid size-11 place-items-center rounded-lg border border-[var(--hairline-strong)] bg-[var(--surface-raised)] shadow-[var(--shadow-sm)]">
          <span className="font-display text-xs tracking-normal">AF</span>
        </div>
      </div>

      {/* 上游渠道节点 */}
      <div className="absolute inset-y-0 right-5 flex flex-col justify-around py-3">
        {targets.map((logo) => (
          <div
            key={logo.name}
            className="grid size-9 place-items-center rounded-lg border border-[var(--hairline)] bg-[var(--surface-raised)] text-muted-foreground"
            title={logo.name}
          >
            <logo.Icon className="size-4.5" />
          </div>
        ))}
      </div>
    </div>
  )
}
