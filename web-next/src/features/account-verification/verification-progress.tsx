import { CheckCircle2, Clock3, XCircle, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { VerificationKind } from './account-verification-api'

type StageState = 'completed' | 'active' | 'pending' | 'failed'

const stageKeys = ['submitted', 'review', 'done'] as const

/** 提交资料 → 审核 → 完成；status 与后台审核状态码一一对应。 */
function stageStates(status: number, providerStatus?: string | null): StageState[] {
  if (providerStatus === 'expired') return ['completed', 'failed', 'pending']
  if (status === 4) return ['completed', 'completed', 'completed']
  if (status === 5) return ['completed', 'completed', 'failed']
  if (status === 1 || status === 2 || status === 3) return ['completed', 'active', 'pending']
  return ['active', 'pending', 'pending']
}

const stateIcon: Record<StageState, LucideIcon> = {
  completed: CheckCircle2,
  active: Clock3,
  pending: CheckCircle2,
  failed: XCircle,
}

/** 进度条：三项圆点 + 连接线，整体横向排布，状态色与后台审核状态一致。 */
export function VerificationProgress({ status, providerStatus, approved = false }: { status: number; providerStatus?: string | null; approved?: boolean }) {
  const { t } = useTranslation()
  const steps = t(approved ? 'verificationCenter.progressStepsApproved' : 'verificationCenter.progressSteps', { returnObjects: true }) as unknown as Record<string, string>
  const states = stageStates(status, providerStatus)
  return (
    <ol className="grid w-full grid-cols-3" aria-label={t('verificationCenter.progress')}>
      {states.map((state, index) => {
        const Icon = stateIcon[state]
        return (
          <li key={stageKeys[index]} aria-current={state === 'active' ? 'step' : undefined} className="relative flex flex-col items-center gap-1.5">
            {/* 连接线：从前一个圆点中心连到当前圆点中心，两端留出圆点半径，压到圆点下方。 */}
            {index > 0 ? <span aria-hidden="true" className={cn('absolute right-1/2 top-[0.875rem] h-px w-full -translate-x-3.5', states[index - 1] === 'pending' ? 'bg-default-200' : 'bg-success/50')} /> : null}
            <span className={cn(
              'relative z-10 grid size-7 place-items-center rounded-full border',
              state === 'completed' && 'border-success/30 bg-success/10 text-success',
              state === 'active' && 'border-info/30 bg-info/10 text-info',
              state === 'failed' && 'border-danger/30 bg-danger/10 text-danger',
              state === 'pending' && 'border-default-200 bg-content1 text-default-300',
            )}>
              {state === 'pending'
                ? <span className="size-1.5 rounded-full bg-current" aria-hidden="true" />
                : <Icon className={cn('size-3.5', state === 'active' && 'animate-pulse')} aria-hidden="true" />}
            </span>
            <span className={cn(
              'whitespace-nowrap text-xs',
              state === 'completed' && 'text-success',
              state === 'active' && 'font-medium text-info',
              state === 'failed' && 'text-danger',
              state === 'pending' && 'text-default-400',
            )}>{steps[stageKeys[index]]}</span>
          </li>
        )
      })}
    </ol>
  )
}

/** 信息卡：与后台"认证信息/提交信息"区块一致，三列栅格展示主体与时间。 */
export function VerificationInfoCard({
  id,
  kind,
  subjectName,
  documentType,
  documentNumberMasked,
  updatedAt,
}: {
  id: number
  kind: VerificationKind
  subjectName: string
  documentType?: string | null
  documentNumberMasked?: string | null
  updatedAt?: number
}) {
  const { t, i18n } = useTranslation()
  const time = updatedAt
    ? new Intl.DateTimeFormat(i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }).format(updatedAt * 1000)
    : '—'
  const fields = [
    [t('verificationCenter.infoCaseId'), `#${id}`],
    [t('verificationCenter.subjectName'), subjectName],
    [t('verificationCenter.documentType'), documentType ? t(`verificationCenter.documentTypes.${documentType}`, { defaultValue: documentType }) : '—'],
    [t('verificationCenter.documentNumber'), documentNumberMasked || '—'],
    [t('verificationCenter.infoKind'), t(`verificationCenter.${kind}`)],
    [t('verificationCenter.updated'), time],
  ] as const
  return (
    <div className="rounded-xl border border-default-200 bg-content1 p-4">
      <h4 className="text-xs font-semibold uppercase tracking-[0.08em] text-default-500">{t('verificationCenter.infoTitle')}</h4>
      <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3 sm:grid-cols-3">
        {fields.map(([label, value]) => (
          <div key={label} className="min-w-0">
            <dt className="text-[0.6875rem] text-default-500">{label}</dt>
            <dd className="mt-0.5 truncate text-sm font-medium" title={String(value)}>{value}</dd>
          </div>
        ))}
      </dl>
    </div>
  )
}
