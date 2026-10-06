import type { CSSProperties, ElementType } from 'react'
import { useMemo } from 'react'

import { cn } from '@/lib/utils'

type KineticTextProps = {
  /** 要动效呈现的文本。拆字仅作视觉层，语义由 aria-label 承载。 */
  text: string
  as?: ElementType
  className?: string
  /** 整体入场延迟（秒） */
  delay?: number
  /** 逐字间隔（秒），设 0 则整体同时入场 */
  stagger?: number
  /** 悬停时单字放大偏转，用于 Hero 这类可玩性强的位置 */
  interactive?: boolean
  /** 指定序号的字符染成品牌红/交互蓝，做「标点式」强调 */
  accentAt?: readonly number[]
}

// 空格用不换行空格渲染，避免 inline-block 拆分后空白被折叠。
const NBSP = ' '

export function KineticText({
  text,
  as: Component = 'span',
  className,
  delay = 0,
  stagger = 0.035,
  interactive = false,
  accentAt,
}: KineticTextProps) {
  // 先按词分组再拆字：词内不换行，整体仍能正常折行。
  const words = useMemo(() => splitIntoWords(text), [text])
  const accents = useMemo(() => new Set(accentAt ?? []), [accentAt])

  let charCursor = -1

  return (
    <Component className={cn('font-display', className)} aria-label={text}>
      {words.map((word, wordIndex) => (
        <span key={`${word}-${wordIndex}`} className="inline-block whitespace-nowrap" aria-hidden="true">
          {[...word].map((char, charIndex) => {
            charCursor += 1

            return (
              <span
                key={`${char}-${charIndex}`}
                className={cn(
                  'inline-block animate-kinetic-rise will-change-transform',
                  // 紧字距下不做旋转/放大，否则字母会互撞；只抬升并提亮。
                  interactive &&
                    'transition-[transform,color] duration-300 hover:-translate-y-1.5 hover:text-brand',
                  accents.has(charCursor) && 'text-brand',
                )}
                style={{ animationDelay: `${delay + charCursor * stagger}s` } satisfies CSSProperties}
                data-kinetic-char=""
              >
                {char === ' ' ? NBSP : char}
              </span>
            )
          })}
        </span>
      ))}
    </Component>
  )
}

/**
 * 拆分为「不可折行的单元」：拉丁词整体保留（含尾随空格），
 * CJK 逐字独立成组，保持中文可在任意字间折行的排版特性。
 */
function splitIntoWords(text: string): string[] {
  // CJK 统一表意文字、假名、全角标点
  const CJK = /[　-鿿＀-￯]/u
  const units: string[] = []

  for (const chunk of text.split(/(?<=\s)/u)) {
    if (chunk.length === 0) {
      continue
    }

    if (CJK.test(chunk)) {
      units.push(...chunk)
      continue
    }

    units.push(chunk)
  }

  return units
}
