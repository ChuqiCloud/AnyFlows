import { Languages } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'

export function LanguageToggle() {
  const { i18n, t } = useTranslation()
  const useChinese = i18n.language.startsWith('zh')
  const nextLanguage = useChinese ? 'en' : 'zh'

  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="gap-2"
      aria-label={t('language.toggle')}
      title={t('language.toggle')}
      onClick={() => void i18n.changeLanguage(nextLanguage)}
    >
      <Languages className="size-4" aria-hidden="true" />
      {nextLanguage.toUpperCase()}
    </Button>
  )
}
