import { useQuery } from '@tanstack/react-query'
import { ArrowRight } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { merchantGet } from './merchant-pool-api'

type MerchantAttempt = { merchant_id: number; merchant_name: string; service_id: number; service_name: string; outcome: 'success' | 'failure' | 'ignored'; latency_ms: number; completed_at: number; request_id: string }

export function MerchantAttemptPath({ requestId }: { requestId: string }) {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ['merchant-pool', 'attempts', requestId], queryFn: ({ signal }) => merchantGet<{ items: MerchantAttempt[] }>('/api/merchant-pool/attempts', signal, { request_id: requestId }), staleTime: 15_000, retry: false, refetchInterval: 30_000, refetchIntervalInBackground: false })
  if (!query.data?.items.length) return null
  return <section className="border-b border-[var(--hairline)] py-4"><h3 className="mb-3 text-xs font-semibold">{t('merchantPool.attemptPath')}</h3><ol className="flex flex-wrap items-center gap-2 text-xs">{query.data.items.map((attempt, index) => <li key={`${attempt.service_id}-${index}`} className="flex items-center gap-2">{index > 0 ? <ArrowRight className="size-3.5 text-muted-foreground" aria-hidden="true" /> : null}<div className="rounded-lg bg-surface-2 px-2 py-1.5"><span className="font-medium">{attempt.merchant_name}</span><span className={attempt.outcome === 'success' ? 'ml-2 text-success' : attempt.outcome === 'failure' ? 'ml-2 text-danger' : 'ml-2 text-muted-foreground'}>{t(`merchantPool.attemptOutcomes.${attempt.outcome}`)}</span><p className="mt-1 text-[0.6875rem] text-muted-foreground">{attempt.service_name} · {Math.round(attempt.latency_ms).toLocaleString()} ms</p></div></li>)}</ol></section>
}
