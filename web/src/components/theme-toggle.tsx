import { Moon, Sun } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'

type ThemeMode = 'light' | 'dark'

const storageKey = 'anyflows.theme'

function getInitialTheme(): ThemeMode {
  const stored = window.localStorage.getItem(storageKey)

  if (stored === 'light' || stored === 'dark') {
    return stored
  }

  // 深色是正统形态，仅在系统明确偏好浅色时才默认浅色。
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

export function ThemeToggle() {
  const { t } = useTranslation()
  const [theme, setTheme] = useState<ThemeMode>(getInitialTheme)
  const isDark = theme === 'dark'

  useEffect(() => {
    const root = document.documentElement

    // 两个类都显式写入：token 定义在 .dark/.light 上，dark: 变体也依赖 .dark。
    root.classList.toggle('dark', isDark)
    root.classList.toggle('light', !isDark)
    window.localStorage.setItem(storageKey, theme)
  }, [isDark, theme])

  return (
    <Button
      type="button"
      variant="outline"
      size="icon"
      aria-label={t('theme.toggle')}
      title={t('theme.toggle')}
      onClick={() => setTheme(isDark ? 'light' : 'dark')}
    >
      {isDark ? <Sun className="size-4" aria-hidden="true" /> : <Moon className="size-4" aria-hidden="true" />}
    </Button>
  )
}
