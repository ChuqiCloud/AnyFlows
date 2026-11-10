import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { apiClient } from '@/lib/api'
import { listAdminCredentials } from '@/lib/api/generated/sdk.gen'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { capabilityGet, credentialUrl, type ModelObservation } from './key-capability-api'
import { CapabilityError } from './key-capability-fields'

export function ChannelModelCoverage({ channelId, models }: { channelId: number; models: readonly string[] }) {
  const { t } = useTranslation()
  const [enabled, setEnabled] = useState(false)
  const query = useQuery({
    queryKey: ['upstream-key-capabilities', channelId, 'coverage'],
    enabled,
    refetchOnWindowFocus: false,
    queryFn: async ({ signal }) => {
      const keys: AdminCredential[] = []
      const cursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminCredentials({ client: apiClient, path: { channel_id: channelId }, query: { after, limit: 100 }, signal })
        keys.push(...data.credentials)
        const next = data.next_cursor ?? undefined
        if (next === undefined) break
        if (cursors.has(next)) throw new Error('Repeated credential cursor')
        cursors.add(next); after = next
      }
      const records: Array<{ id: number; allowed: string[] | null }> = []
      let cursor = 0
      const read = async () => {
        while (cursor < keys.length && !signal.aborted) {
          const credential = keys[cursor++]
          try {
            const snapshot = await capabilityGet<ModelObservation>(`${credentialUrl(channelId, credential.id)}/models`, signal)
            // The server combines published range, channel/parent/Key policy and status.
            // A local state guard also prevents disabled or paused credentials being counted.
            const ready = credential.status === 'enabled' && credential.schedulable && !credential.oauth_token_pending
            const allowed = ready ? [...new Set(snapshot.items.filter((item) => item.enabled).flatMap((item) => item.canonical_models))] : []
            records.push({ id: credential.id, allowed })
          } catch (error) {
            if (signal.aborted) throw error
            records.push({ id: credential.id, allowed: null })
          }
        }
      }
      await Promise.all([read(), read()])
      const counts: Record<string, number> = {}
      for (const record of records) for (const model of record.allowed ?? []) counts[model] = (counts[model] ?? 0) + 1
      return { counts, failed: records.filter((item) => item.allowed === null).length, total: keys.length }
    },
  })
  const reload = () => { if (!enabled) setEnabled(true); else void query.refetch() }
  const pending = enabled && query.isFetching
  const error = query.isError
  const data = query.data
  const union = data ? models.filter((model) => (data.counts[model] ?? 0) > 0) : []
  return <section className="grid min-w-0 gap-3 border-y border-[var(--hairline)] py-4" aria-label={t('keyCapabilities.coverageTitle')}><div className="flex flex-wrap items-center justify-between gap-2"><h3 className="text-sm font-semibold">{t('keyCapabilities.coverageTitle')}</h3><Button type="button" size="sm" variant="bordered" isLoading={pending} isDisabled={pending} onPress={() => void reload()}><RefreshCw className="size-3.5" />{t(enabled ? 'keyCapabilities.refreshCoverage' : 'keyCapabilities.readCoverage')}</Button></div><p className="text-[11px] leading-5 text-muted-foreground">{t('keyCapabilities.coverageHint')}</p>
    {error ? <CapabilityError onRetry={() => void reload()} /> : null}
    {data ? <><p role="status" className="text-xs text-muted-foreground">{t(data.failed ? 'keyCapabilities.partialUnion' : 'keyCapabilities.modelUnion', { count: union.length, total: data.total, failed: data.failed })}</p>{data.failed ? <p role="alert" className="text-xs text-warning">{t('keyCapabilities.partialCoverage', { count: data.failed })}</p> : null}<div className="max-h-72 overflow-auto"><table className="w-full table-fixed text-left text-xs"><thead className="text-[11px] text-muted-foreground"><tr><th className="py-2 pr-3">{t('keyCapabilities.publishedModel')}</th><th className="w-36 py-2">{t('keyCapabilities.supportingKeys')}</th></tr></thead><tbody>{models.map((model) => <tr key={model} className="border-t border-[var(--hairline)]"><td className="break-all py-2 pr-3 font-medium">{model}</td><td className="py-2 tabular-nums">{data.failed || error ? (data.counts[model] ?? 0) > 0 ? t('keyCapabilities.atLeastKeys', { count: data.counts[model] }) : t('keyCapabilities.unknown') : t('keyCapabilities.keyCount', { count: data.counts[model] ?? 0 })}</td></tr>)}</tbody></table>{models.length === 0 ? <p className="py-3 text-xs text-muted-foreground">{t('keyCapabilities.noPublishedModels')}</p> : null}</div></> : enabled && !error ? <p role="status" className="text-xs text-muted-foreground">{t('keyCapabilities.loading')}</p> : null}
  </section>
}
