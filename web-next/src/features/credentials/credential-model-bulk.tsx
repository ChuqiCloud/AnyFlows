import { useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Button } from '@heroui/react'
import { ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { capabilityKey, refreshCredentialModels } from './key-capability-api'

type Result = { id: number; status: 'waiting' | 'running' | 'success' | 'empty' | 'failed'; count?: number }
export function CredentialModelBulk({ channelId, credentials, selectedIds, ready }: { channelId: number; credentials: readonly AdminCredential[]; selectedIds: readonly number[]; ready: boolean }) {
  const { t } = useTranslation()
  const client = useQueryClient()
  const [pending, setPending] = useState(false)
  const [results, setResults] = useState<Result[]>([])
  const enabledIds = credentials.filter((item) => item.status === 'enabled' && !item.oauth_token_pending).map((item) => item.id)
  const selected = credentials.filter((item) => selectedIds.includes(item.id)).map((item) => item.id)
  const run = async (ids: number[]) => {
    if (!ids.length || pending) return
    setPending(true)
    setResults(ids.map((id) => ({ id, status: 'waiting' })))
    // A bounded sequential request keeps each result bound to its own Key.
    for (const id of ids) {
      setResults((items) => items.map((item) => item.id === id ? { id, status: 'running' } : item))
      try {
        const data = await refreshCredentialModels(channelId, id)
        client.setQueryData([...capabilityKey(channelId, id), 'models'], data)
        client.setQueryData([...capabilityKey(channelId, id), 'policy'], data)
        const status = data.status === 'available' ? 'success' : data.status === 'empty' ? 'empty' : 'failed'
        setResults((items) => items.map((item) => item.id === id ? { id, status, count: data.items.length } : item))
      } catch {
        setResults((items) => items.map((item) => item.id === id ? { id, status: 'failed' } : item))
      }
    }
    await client.invalidateQueries({ queryKey: ['model-catalog'] })
    setPending(false)
  }
  const failed = results.filter((item) => item.status === 'failed').map((item) => item.id)
  return <div className="basis-full border-y border-[var(--hairline)] py-2.5"><div className="flex flex-wrap items-center gap-2"><Button type="button" size="sm" variant="bordered" isDisabled={!ready || pending || selected.length === 0} onPress={() => void run(selected)}><ScanSearch className="size-3.5" />{t('keyCapabilities.fetchSelected', { count: selected.length })}</Button><Button type="button" size="sm" variant="light" isDisabled={!ready || pending || enabledIds.length === 0} onPress={() => void run(enabledIds)}>{t('keyCapabilities.fetchEnabled', { count: enabledIds.length })}</Button>{failed.length ? <Button type="button" size="sm" variant="light" isDisabled={pending} onPress={() => void run(failed)}>{t('keyCapabilities.retryFailed', { count: failed.length })}</Button> : null}{results.length ? <span role="status" className="text-[11px] text-muted-foreground">{t('keyCapabilities.bulkProgress', { done: results.filter((item) => !['waiting', 'running'].includes(item.status)).length, total: results.length })}</span> : null}</div>{results.length ? <details className="mt-2" open><summary className="cursor-pointer text-[11px] text-muted-foreground">{t('keyCapabilities.singleResults')}</summary><ul className="mt-1 max-h-40 divide-y divide-[var(--hairline)] overflow-auto text-[11px]">{results.map((item) => <li key={item.id} className="flex justify-between gap-2 py-1.5"><span>#{item.id}</span><span className={item.status === 'failed' ? 'text-danger' : 'text-muted-foreground'}>{t(`keyCapabilities.bulk.${item.status}`, { count: item.count ?? 0 })}</span></li>)}</ul></details> : null}</div>
}
