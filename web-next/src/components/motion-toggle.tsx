import { Button } from '@heroui/react'
import { Sparkle, Sparkles } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

type MotionMode = 'full' | 'reduced'

const storageKey = 'anyflows.motion'

/**
 * 未选择过时跟随系统：系统要求减少动态效果就默认 reduced，否则 full。
 * 选择过之后用户意图优先——无障碍偏好是默认值，不是不可翻越的墙。
 */
function getInitialMotion(): MotionMode {
  const stored = window.localStorage.getItem(storageKey)

  if (stored === 'full' || stored === 'reduced') {
    return stored
  }

  return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'reduced' : 'full'
}

export function MotionToggle() {
  const { t } = useTranslation()
  const [motion, setMotion] = useState<MotionMode>(getInitialMotion)
  const isFull = motion === 'full'

  useEffect(() => {
    document.documentElement.setAttribute('data-motion', motion)
    window.localStorage.setItem(storageKey, motion)
  }, [motion])

  const label = isFull ? t('motion.disable') : t('motion.enable')

  return (
    <Button
      isIconOnly
      aria-label={label}
      aria-pressed={isFull}
      size="md"
      title={label}
      type="button"
      variant="bordered"
      onClick={() => setMotion(isFull ? 'reduced' : 'full')}
    >
      {isFull ? (
        <Sparkles className="size-4" aria-hidden="true" />
      ) : (
        <Sparkle className="size-4" aria-hidden="true" />
      )}
    </Button>
  )
}
