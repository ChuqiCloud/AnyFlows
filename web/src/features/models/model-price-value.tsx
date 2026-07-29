import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

type ModelPriceValueProps = {
  label: string
  value?: string
  muted?: boolean
}

/** 价格文本保持服务端十进制原值，禁止经过 Number 或浮点格式化。 */
export function ModelPriceValue({ label, value, muted = false }: ModelPriceValueProps) {
  const { t } = useTranslation()
  if (value === undefined) {
    return <span className="text-muted-foreground">{t('models.values.notApplicable')}</span>
  }
  return (
    <span
      title={t('models.values.priceTitle', { label, value })}
      className={cn('block min-w-0 tabular-nums', muted && 'text-muted-foreground')}
    >
      <span className="block truncate font-mono text-[0.75rem] text-foreground">${value}</span>
      <span className="mt-0.5 block text-[0.625rem] text-muted-foreground">
        {t('models.values.perMillion')}
      </span>
    </span>
  )
}
