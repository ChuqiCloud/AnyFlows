import { useTranslation } from 'react-i18next'

/** 阶梯定价示意：区间与倍率均为展示用例，不代表实际价格 */
const tiers = [
  { range: '0 – 1M', ratio: '1.00', width: '38%' },
  { range: '1M – 10M', ratio: '0.85', width: '62%' },
  { range: '10M+', ratio: '0.70', width: '100%' },
] as const

/**
 * 计费可视化：三档阶梯倍率的条形示意。
 *
 * 与 RouteVisual 一样是静态示意图，不接真实数据——但它比一段文字更快
 * 讲清「用量越大倍率越低」这件事。
 */
export function BillingVisual() {
  const { t } = useTranslation()

  return (
    <div className="mt-8 space-y-3.5" aria-label={t('landing.feature.billing.title')}>
      {tiers.map((tier) => (
        <div key={tier.range}>
          <div className="flex items-baseline justify-between font-mono text-[0.6875rem] text-muted-foreground/70 tabular-nums">
            <span>{tier.range}</span>
            <span className="text-foreground/80">×{tier.ratio}</span>
          </div>
          <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-[var(--surface-sunken)]">
            <div
              className="h-full rounded-full bg-gradient-to-r from-brand/35 to-brand/80"
              style={{ width: tier.width }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}
