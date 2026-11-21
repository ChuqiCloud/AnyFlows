import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

type ModelPriceValueProps = {
  label: string
  value?: string
  muted?: boolean
  showLabel?: boolean
  unit?: 'per_million' | 'per_request'
}

/** 价格文本保持服务端十进制原值，禁止经过 Number 或浮点格式化。 */
export function ModelPriceValue({ label, value, muted = false, showLabel = false, unit = 'per_million' }: ModelPriceValueProps) {
  const { t } = useTranslation()
  if (value === undefined) {
    return (
      <span className="block text-muted-foreground">
        {showLabel ? <span className="mb-0.5 block text-[0.625rem]">{label}</span> : null}
        {t('models.values.notApplicable')}
      </span>
    )
  }
  return (
    <span
      title={t(unit === 'per_request' ? 'models.values.requestPriceTitle' : 'models.values.priceTitle', { label, value })}
      className={cn('block min-w-0 tabular-nums', muted && 'text-muted-foreground')}
    >
      {showLabel ? <span className="mb-0.5 block text-[0.625rem] text-muted-foreground">{label}</span> : null}
      {showLabel ? (
        <span className="flex min-w-0 items-baseline gap-1">
          <span className="min-w-0 truncate font-mono text-[0.75rem] text-foreground">${value}</span>
          <span className="shrink-0 text-[0.625rem] text-muted-foreground">{t(unit === 'per_request' ? 'models.values.perRequestCompact' : 'models.values.perMillionCompact')}</span>
        </span>
      ) : (
        <>
          <span className="block truncate font-mono text-[0.75rem] text-foreground">${value}</span>
          <span className="mt-0.5 block text-[0.625rem] text-muted-foreground">{t(unit === 'per_request' ? 'models.values.perRequest' : 'models.values.perMillion')}</span>
        </>
      )}
    </span>
  )
}
