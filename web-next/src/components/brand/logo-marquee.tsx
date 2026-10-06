import { modelLogos } from '@/components/brand/model-logos'
import { cn } from '@/lib/utils'

type LogoMarqueeProps = {
  className?: string
  /**
   * 单轮时长（秒）。默认值偏大是刻意的：这条带子是背景信息而非视觉焦点，
   * 慢到几乎察觉不出在动，视线才不会被它拽走。
   */
  duration?: number
  reverse?: boolean
}

/**
 * 模型标识滚动条。
 *
 * 降级形态（居中静态排布、隐藏第二份副本）全部由 index.css 里的 data-motion
 * 规则驱动，这里不写 motion-reduce: 变体——那个变体只认系统偏好，站内的
 * data-motion="full" 无法覆盖它，会导致用户开了动效轨道依然不滚。
 */
export function LogoMarquee({ className, duration = 88, reverse = false }: LogoMarqueeProps) {
  // 关键帧位移 -50%，必须正好两份内容才能无缝循环。
  const track = [...modelLogos, ...modelLogos]

  return (
    <div
      className={cn(
        'group/marquee relative flex overflow-hidden',
        '[mask-image:linear-gradient(to_right,transparent,black_12%,black_88%,transparent)]',
        className,
      )}
      role="presentation"
      aria-hidden="true"
    >
      <div
        className={cn(
          'flex w-max shrink-0 animate-marquee items-center gap-14',
          reverse && '[animation-direction:reverse]',
          'group-hover/marquee:[animation-play-state:paused]',
        )}
        style={{ animationDuration: `${duration}s` }}
        data-marquee-track=""
      >
        {track.map((logo, index) => (
          <span
            key={`${logo.name}-${index}`}
            className="shrink-0 text-muted-foreground/55 transition-colors duration-300 hover:text-foreground"
            title={logo.name}
            {...(index >= modelLogos.length ? { 'data-marquee-clone': '' } : {})}
          >
            <logo.Icon className="size-8" />
          </span>
        ))}
      </div>
    </div>
  )
}
