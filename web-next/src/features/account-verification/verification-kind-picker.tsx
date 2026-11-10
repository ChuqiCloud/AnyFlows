import { ArrowRight, Building2, Clock3, Crown, FileText, Percent, ShieldCheck, UserRound, Users, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { VerificationKind } from './account-verification-api'

type KindBenefit = { icon: LucideIcon; text: string }

/*
 * 未认证首屏：两种认证类型并排供选择。
 * 卡片保持中性表面，只靠图标颜色区分类型；选中态用主色边框 + 淡主色底，不叠加渐变或阴影。
 */
export function VerificationKindPicker({ kind, onPick }: { kind: VerificationKind; onPick: (kind: VerificationKind) => void }) {
  const { t } = useTranslation()
  const features = ([
    { icon: ShieldCheck, text: t('verificationCenter.kindFeatures.security') },
    { icon: Crown, text: t('verificationCenter.kindFeatures.privilege') },
    { icon: Users, text: t('verificationCenter.kindFeatures.support') },
  ] as const).map((item) => <span key={item.text} className="inline-flex items-center gap-1.5 rounded-full bg-default-100 px-2.5 py-1 text-xs text-default-600"><item.icon className="size-3.5 text-default-400" aria-hidden="true" />{item.text}</span>)
  const options: Array<{ key: VerificationKind; icon: LucideIcon; tone: string; benefits: KindBenefit[] }> = [
    {
      key: 'individual',
      icon: UserRound,
      tone: 'text-primary',
      benefits: [
        { icon: Clock3, text: t('verificationCenter.kindBenefits.fast') },
        { icon: ShieldCheck, text: t('verificationCenter.kindBenefits.safe') },
        { icon: Crown, text: t('verificationCenter.kindBenefits.discount') },
      ],
    },
    {
      key: 'enterprise',
      icon: Building2,
      tone: 'text-info',
      benefits: [
        { icon: FileText, text: t('verificationCenter.kindBenefits.invoice') },
        { icon: Users, text: t('verificationCenter.kindBenefits.support') },
        { icon: Percent, text: t('verificationCenter.kindBenefits.enterpriseDiscount') },
      ],
    },
  ]

  return (
    <section className="grid gap-5 rounded-2xl border border-default-200 bg-content1 p-6">
      <header className="text-center">
        <h2 className="text-lg font-semibold">{t('verificationCenter.kindPickerTitle')}</h2>
        <p className="mt-1.5 text-sm leading-6 text-default-500">{t('verificationCenter.kindPickerHint')}</p>
        <div className="mt-3 flex flex-wrap justify-center gap-2">{features}</div>
      </header>

      <div className="grid gap-4 sm:grid-cols-2">
        {options.map(({ key, icon: Icon, tone, benefits }) => {
          const active = kind === key
          return (
            <button
              key={key}
              type="button"
              aria-pressed={active}
              onClick={() => onPick(key)}
              className={cn(
                'group flex flex-col gap-3 rounded-2xl border p-5 text-left transition-colors outline-none',
                'focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-content1',
                active ? 'border-primary bg-primary/[0.04]' : 'border-default-200 hover:border-default-300 hover:bg-default-50',
              )}
            >
              <span className={cn('grid size-11 place-items-center rounded-xl bg-default-100', tone)}>
                <Icon className="size-5" aria-hidden="true" />
              </span>
              <div>
                <h3 className="text-base font-semibold">{t(`verificationCenter.${key}`)}</h3>
                <p className="mt-1 text-sm leading-6 text-default-500">{t(`verificationCenter.kindDetails.${key}`)}</p>
              </div>
              <ul className="flex flex-wrap gap-1.5">
                {benefits.map(({ icon: BenefitIcon, text }) => (
                  <li key={text} className="inline-flex items-center gap-1 rounded-full bg-default-100 px-2.5 py-1 text-xs text-default-600">
                    <BenefitIcon className="size-3.5 text-default-400" aria-hidden="true" />{text}
                  </li>
                ))}
              </ul>
              <p className="mt-auto border-t border-default-200 pt-3 text-xs leading-5 text-default-400">{t(`verificationCenter.kindTips.${key}`)}</p>
              <span className={cn(
                'inline-flex items-center justify-center gap-1.5 rounded-full px-3.5 py-1.5 text-sm font-medium transition-colors',
                active ? 'bg-primary text-primary-foreground' : 'bg-default-100 text-default-700 group-hover:bg-default-200',
              )}>
                {t(active ? 'verificationCenter.kindSelected' : 'verificationCenter.kindStart')}
                <ArrowRight className="size-4" aria-hidden="true" />
              </span>
            </button>
          )
        })}
      </div>

      {/* 政策提示条：仅用于身份核验，对应后台"认证信息仅用于身份验证"的说明。 */}
      <p className="rounded-xl bg-default-100 px-4 py-3 text-center text-xs leading-5 text-default-500">
        {t('verificationCenter.privacyPolicy')}
      </p>
    </section>
  )
}
