import { Button } from '@heroui/react'
import { Languages } from 'lucide-react'
import { useTranslation } from 'react-i18next'

export function LanguageToggle() {
  const { i18n, t } = useTranslation()
  const useChinese = i18n.language.startsWith('zh')
  const nextLanguage = useChinese ? 'en' : 'zh'

  return (
    <Button
      aria-label={t('language.toggle')}
      className="gap-2"
      size="sm"
      title={t('language.toggle')}
      type="button"
      variant="bordered"
      onClick={() => void i18n.changeLanguage(nextLanguage)}
    >
      <Languages className="size-4" aria-hidden="true" />
      {nextLanguage.toUpperCase()}
    </Button>
  )
}
