import { useEffect, useState } from 'react'

import { BrandMark } from '@/components/layout/brand-mark'
import { cn } from '@/lib/utils'

type SiteBrandMarkProps = {
  className?: string
  logoUrl?: string | null
  siteName: string
  size?: 'nav' | 'hero'
}

/** 优先呈现运营配置的 Logo，加载失败时稳定回退到内置品牌标识。 */
export function SiteBrandMark({
  className,
  logoUrl,
  siteName,
  size = 'nav',
}: SiteBrandMarkProps) {
  const [failed, setFailed] = useState(false)

  useEffect(() => setFailed(false), [logoUrl])

  if (!logoUrl || failed) {
    return <BrandMark className={className} size={size} />
  }

  return (
    <span
      className={cn(
        'grid shrink-0 place-items-center overflow-hidden border border-[var(--hairline)] bg-surface-1',
        size === 'nav' ? 'size-8 rounded-lg' : 'size-14 rounded-2xl',
        className,
      )}
    >
      <img
        src={logoUrl}
        alt={siteName}
        className="size-full object-contain"
        decoding="async"
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
      />
    </span>
  )
}
