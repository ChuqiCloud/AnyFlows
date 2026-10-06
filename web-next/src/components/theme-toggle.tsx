import { Button } from '@heroui/react'
import { Moon, Sun } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useSiteTheme } from '@/shared/components/use-site-theme'

export function ThemeToggle() {
  const { t } = useTranslation()
  const { isDark, setTheme } = useSiteTheme()

  return (
    <Button
      isIconOnly
      aria-label={t('theme.toggle')}
      size="md"
      title={t('theme.toggle')}
      type="button"
      variant="bordered"
      onClick={() => setTheme(isDark ? 'light' : 'dark')}
    >
      {isDark ? <Sun className="size-4" aria-hidden="true" /> : <Moon className="size-4" aria-hidden="true" />}
    </Button>
  )
}
