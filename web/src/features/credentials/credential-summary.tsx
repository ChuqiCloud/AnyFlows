import { CircleCheck, CircleOff, Clock3, KeyRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminCredential } from '@/lib/api/generated/types.gen'
import {
  credentialRuntimeState,
  isSparkShadowCredential,
  sparkShadowParentBlocked,
} from './credential-model'

export function CredentialSummary({ credentials }: { credentials: readonly AdminCredential[] }) {
  const { t } = useTranslation()
  const counts = credentials.reduce((result, credential) => {
    const state = credentialRuntimeState(credential)
    const parent = credential.parent_id
      ? credentials.find((item) => item.id === credential.parent_id)
      : undefined
    const sharedBlocked = isSparkShadowCredential(credential)
      && sparkShadowParentBlocked(parent)
    result.total += 1
    if (state === 'available' && !sharedBlocked) result.available += 1
    else if (state === 'cooling') result.cooling += 1
    else result.unavailable += 1
    return result
  }, { total: 0, available: 0, cooling: 0, unavailable: 0 })
  const items = [
    { key: 'total', value: counts.total, icon: KeyRound, color: 'text-foreground' },
    { key: 'available', value: counts.available, icon: CircleCheck, color: 'text-success' },
    { key: 'cooling', value: counts.cooling, icon: Clock3, color: 'text-warning' },
    { key: 'unavailable', value: counts.unavailable, icon: CircleOff, color: 'text-muted-foreground' },
  ] as const
  return (
    <div className="grid grid-cols-2 border-y border-[var(--hairline)] sm:grid-cols-4">
      {items.map((item) => {
        const Icon = item.icon
        return (
          <div key={item.key} className="flex min-h-16 items-center gap-3 border-r border-b border-[var(--hairline)] px-3 last:border-r-0 sm:border-b-0">
            <Icon className={`size-4 shrink-0 ${item.color}`} aria-hidden="true" />
            <div><p className="text-base font-semibold tabular-nums">{item.value}</p><p className="text-[0.6875rem] text-muted-foreground">{t(`credentials.summary.${item.key}`)}</p></div>
          </div>
        )
      })}
    </div>
  )
}
