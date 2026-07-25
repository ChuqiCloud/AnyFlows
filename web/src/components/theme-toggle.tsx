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

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function ThemeToggle() {
  const { t } = useTranslation()
  const [theme, setTheme] = useState<ThemeMode>(getInitialTheme)
  const isDark = theme === 'dark'

  useEffect(() => {
    const root = document.documentElement

    // 手动切换主题时同步根节点 class，保证 Tailwind dark 变体和 shadcn token 同源。
    root.classList.toggle('dark', isDark)
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
