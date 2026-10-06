import { useEffect, useRef } from 'react'

import { modelLogos } from '@/components/brand/model-logos'
import { cn } from '@/lib/utils'

/**
 * 探照遮罩：中心实、边缘透，让顶层只在光斑处显影。
 * 抽成常量是因为 maskImage / WebkitMaskImage 必须逐字一致，否则两端行为会分叉。
 */
const SPOT_MASK =
  'radial-gradient(circle var(--spot-r) at var(--spot-x) var(--spot-y), #000 0%, rgb(0 0 0 / 0.55) 42%, transparent 74%)'

type LogoFieldProps = {
  className?: string
  /** 重复铺排的遍数，决定星座密度 */
  repeat?: number
  /** 探照光半径（px） */
  radius?: number
}

/**
 * 静默扫光的半径倍数。强度只有指针光斑的四成左右，若还用同样半径会暗到看不见；
 * 放大光斑换取可见性，同时保持"弥散的环境光"而非"聚焦的探照灯"的观感。
 */
const SWEEP_RADIUS_SCALE = 1.35

/** 指针离开视口后光斑淡出，常态是完全不可见的 */
const HIDDEN_ALPHA = '0'

/**
 * 阻尼以"半衰期"表示：每经过这段时间，当前值与目标值的差距缩小一半（ms）。
 *
 * 用半衰期而非"每帧推进比例"，是为了让手感与刷新率无关——按帧插值时，
 * 144Hz 屏每秒迭代的次数是 60Hz 的两倍多，同一个系数会明显更快。
 */
const POINTER_HALF_LIFE = 52

/**
 * 静默扫光与透明度沿用偏重的阻尼。扫光目标点本身移动极慢，
 * 这里的阻尼主要决定淡入淡出的柔和度，跟快不快无关。
 */
const AMBIENT_HALF_LIFE = 130

/** 半衰期 → 本帧插值系数。dt 越大推进越多，故与帧率解耦。 */
const easeFactor = (halfLife: number, dt: number) => 1 - Math.pow(2, -dt / halfLife)

/**
 * 静默扫光：一轮从左到右的时长（ms）。26s 意味着光斑横穿一屏要将近半分钟，
 * 慢到不会争夺注意力——用户察觉到的是"背景在轻微呼吸"而非"有东西在动"。
 */
const SWEEP_PERIOD = 26000

/**
 * 一个周期中真正在扫光的比例，其余为全暗间歇。
 * 0.62 → 约 16s 扫过、10s 空档，"隐"的时间足够长才谈得上若隐若现。
 */
const SWEEP_DUTY = 0.62

/** 静默扫光的强度上限，远低于指针光斑（1.0），只求若隐若现 */
const SWEEP_ALPHA = 0.3

/** 指针停止移动多久后交还给静默扫光（ms） */
const IDLE_DELAY = 2600

/**
 * 模型标识背景板：一束柔光把局部「显影」出来，其余部分隐入底色。
 *
 * 光斑有两种驱动源，共用同一层遮罩：
 *  - 指针经过时紧随指针，只留一点阻尼让运动收尾柔和，而非明显的拖尾；
 *  - 指针静止或离开后，交还给一轮极慢的自左向右巡游，只求若隐若现。
 *
 * 位置与强度写进 CSS 变量并由单个 rAF 循环逐帧更新，而非 React state——
 * 否则每帧都会重渲染整棵 logo 子树。
 */
export function LogoField({ className, repeat = 3, radius = 130 }: LogoFieldProps) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = ref.current

    if (!el) {
      return
    }

    // 显式选择优先于系统偏好，与 index.css 的降级链保持同一套判定
    const motion = document.documentElement.dataset.motion

    if (motion === 'reduced') {
      return
    }

    if (motion !== 'full' && window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      return
    }

    // 目标点（指针或扫光）与当前点分离，二者的差值就是"延迟"的来源
    let targetX = 0
    let targetY = 0
    let currentX = 0
    let currentY = 0
    let targetAlpha = 0
    let currentAlpha = 0
    let targetRadius = radius * SWEEP_RADIUS_SCALE
    let currentRadius = targetRadius
    let seeded = false
    let lastMoveAt = 0
    let lastFrameAt = 0
    let frame = 0

    const handleMove = (event: PointerEvent) => {
      const rect = el.getBoundingClientRect()

      targetX = event.clientX - rect.left
      targetY = event.clientY - rect.top
      targetAlpha = 1
      targetRadius = radius
      lastMoveAt = performance.now()

      // 首次拿到指针位置时直接落位，否则光斑会从左上角飞过来
      if (!seeded) {
        seeded = true
        currentX = targetX
        currentY = targetY
      }
    }

    const handleLeave = () => {
      lastMoveAt = 0
    }

    const tick = (now: number) => {
      frame = requestAnimationFrame(tick)

      /*
        首帧无参照，dt 记 0（本帧不推进）；上限 64ms 是为了兜住标签页切回、
        长任务后的巨大 dt——否则 lastFrameAt 隔了几秒，一帧就会把光斑瞬移到位。
      */
      const dt = lastFrameAt === 0 ? 0 : Math.min(now - lastFrameAt, 64)

      lastFrameAt = now

      const rect = el.getBoundingClientRect()
      const pointerActive = lastMoveAt !== 0 && now - lastMoveAt < IDLE_DELAY

      if (!pointerActive) {
        // 静默态：光斑自左向右匀速巡游，纵向停在字标带的视觉中线
        /*
          一个周期里只有前 SWEEP_DUTY 的时间在扫，其余是全暗的间歇。
          没有这段间歇，淡入淡出会首尾相接，背景就成了持续微亮——
          "若隐若现"要求的是有隐有现，而不是一直半亮。
        */
        const cycle = (now % SWEEP_PERIOD) / SWEEP_PERIOD

        if (cycle > SWEEP_DUTY) {
          targetAlpha = 0
          currentAlpha += (targetAlpha - currentAlpha) * easeFactor(AMBIENT_HALF_LIFE, dt)
          el.style.setProperty('--spot-alpha', currentAlpha.toFixed(3))

          // 趁全暗把光斑挪回起跑线，下一轮才不会从上一轮的终点横穿回来
          currentX = -rect.width * 0.12
          currentY = rect.height * 0.42
          el.style.setProperty('--spot-x', `${currentX.toFixed(1)}px`)
          el.style.setProperty('--spot-y', `${currentY.toFixed(1)}px`)

          return
        }

        const phase = cycle / SWEEP_DUTY

        // 略微出界起止，让光斑在板外就已成形，进场时不会出现"半个圆"
        targetX = rect.width * (phase * 1.24 - 0.12)
        targetY = rect.height * 0.42
        targetRadius = radius * SWEEP_RADIUS_SCALE

        /*
          两端淡出，避免光斑在板边缘突然亮起或掐断。
          回卷本身已由上面的全暗间歇兜住，这里只负责进出场的柔和。
        */
        targetAlpha = SWEEP_ALPHA * Math.min(1, Math.min(phase, 1 - phase) / 0.22)
        seeded = true
      }

      /*
        位置的阻尼分两档：跟指针时要贴手，巡游时要飘。
        强度与半径始终走慢档——它们的变化本就该是渐进的，跟快慢无关。
      */
      const moveEase = easeFactor(pointerActive ? POINTER_HALF_LIFE : AMBIENT_HALF_LIFE, dt)
      const ambientEase = easeFactor(AMBIENT_HALF_LIFE, dt)

      currentX += (targetX - currentX) * moveEase
      currentY += (targetY - currentY) * moveEase
      currentAlpha += (targetAlpha - currentAlpha) * ambientEase
      currentRadius += (targetRadius - currentRadius) * ambientEase

      el.style.setProperty('--spot-x', `${currentX.toFixed(1)}px`)
      el.style.setProperty('--spot-y', `${currentY.toFixed(1)}px`)
      el.style.setProperty('--spot-alpha', currentAlpha.toFixed(3))
      el.style.setProperty('--spot-r', `${currentRadius.toFixed(1)}px`)
    }

    // 挂在 window 上：背景板本身 pointer-events:none，只能借全局指针位置定位光斑。
    window.addEventListener('pointermove', handleMove, { passive: true })
    document.addEventListener('pointerleave', handleLeave)
    frame = requestAnimationFrame(tick)

    return () => {
      cancelAnimationFrame(frame)
      window.removeEventListener('pointermove', handleMove)
      document.removeEventListener('pointerleave', handleLeave)
    }
  }, [radius])

  const tiles = Array.from({ length: repeat }, (_, round) =>
    modelLogos.map((logo) => ({ ...logo, key: `${logo.name}-${round}` })),
  ).flat()

  return (
    <div
      ref={ref}
      className={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}
      style={
        {
          // 初始 alpha 为 0，位置不可见，故用什么值都不影响首帧观感
          '--spot-x': '0px',
          '--spot-y': '40%',
          '--spot-alpha': HIDDEN_ALPHA,
          // 与静默态起始半径一致，避免首帧从小突然胀大
          '--spot-r': `${radius * SWEEP_RADIUS_SCALE}px`,
          // 浅色底上深色图标的视觉重量远高于深色底上的浅色图标，强度必须按主题分开给
          '--field-lit': 'var(--logo-field-lit)',
        } as React.CSSProperties
      }
      aria-hidden="true"
    >
      {/*
        只有一层：常态完全不可见，全靠光斑「显影」。
        刻意用前景色而非品牌红——满屏红色 logo 会让红色从「标点」变成主色。

        这里不加 transition-opacity：透明度已由 rAF 逐帧插值，再叠一层 CSS 过渡
        等于两套缓动串联，追随会糊成一团。
      */}
      <LogoGrid
        tiles={tiles}
        className="text-foreground"
        style={{
          opacity: 'calc(var(--spot-alpha) * var(--field-lit))',
          maskImage: SPOT_MASK,
          WebkitMaskImage: SPOT_MASK,
        }}
      />
    </div>
  )
}

type LogoGridProps = {
  tiles: readonly { key: string; name: string; Icon: (typeof modelLogos)[number]['Icon'] }[]
  className?: string
  style?: React.CSSProperties
}

function LogoGrid({ tiles, className, style }: LogoGridProps) {
  return (
    <div
      className={cn(
        'absolute inset-0 grid content-center justify-center gap-x-14 gap-y-12 p-10',
        // auto-fill + minmax：列数随视口自适应，无需断点
        '[grid-template-columns:repeat(auto-fill,minmax(4.5rem,1fr))]',
        className,
      )}
      style={style}
    >
      {tiles.map((tile, index) => (
        <div
          key={tile.key}
          className="grid place-items-center"
          // 逐个错开透明度与尺寸，避免规整网格显得呆板
          style={{ opacity: 0.55 + ((index * 37) % 45) / 100 }}
        >
          <tile.Icon className="size-7 sm:size-8" />
        </div>
      ))}
    </div>
  )
}
