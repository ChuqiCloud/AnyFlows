import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useProxyCleanup, type CleanupRequest, type CleanupResult } from './proxy-pool-api'
import { Button, Checkbox } from '@heroui/react'
import type { ReactNode } from 'react'

export function ProxyCheck({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return <Checkbox size="sm" isSelected={checked} onValueChange={onChange}>{label}</Checkbox>
}
export function ProxySelection({ ids, selected, onChange }: { ids: number[]; selected: number[]; onChange: (ids: number[]) => void }) {
  const { t } = useTranslation()
  const all = ids.length > 0 && ids.every(id => selected.includes(id))
  return <div className="flex flex-wrap items-center gap-3 text-xs"><ProxyCheck label={t('proxyPools.selectVisible')} checked={all} onChange={checked => onChange(checked ? [...new Set([...selected, ...ids])].slice(0, 256) : selected.filter(id => !ids.includes(id)))} /><span>{t('proxyPools.selectedCount', { count: selected.length })}</span></div>
}
export function ProxyCleanupControls({ target, poolId, revision, selected, unused, disabled, onDeleted }: { target: 'pools' | 'nodes'; poolId?: number; revision?: number; selected: number[]; unused?: boolean; disabled?: boolean; onDeleted: () => void }) {
  const { t } = useTranslation()
  const mutation = useProxyCleanup()
  const [preview, setPreview] = useState<{ body: CleanupRequest; result: CleanupResult }>()
  const [continuation, setContinuation] = useState<{ mode: CleanupRequest['mode']; afterId: number }>()
  const [notice, setNotice] = useState('')
  const [stale, setStale] = useState(false)
  const prepare = (mode: CleanupRequest['mode'], afterId?: number) => {
    setPreview(undefined); setContinuation(undefined); setNotice(''); setStale(false); mutation.reset()
    const body: CleanupRequest = { target, mode, pool_id: poolId ?? null, revision: revision ?? null, ids: mode === 'selected' ? selected : [], after_id: afterId ?? null, preview: true }
    mutation.mutate(body, { onSuccess: result => setPreview({ body, result }) })
  }
  const execute = () => {
    if (!preview) return
    mutation.mutate({ ...preview.body, ids: preview.result.eligible_ids, preview: false }, { onSuccess: result => {
      setPreview(undefined)
      setNotice(t('proxyPools.cleanupResult', { deleted: result.deleted_ids.length, protected: result.protected_ids.length, skipped: result.skipped_ids.length }))
      if (preview.result.next_after_id !== null) setContinuation({ mode: preview.body.mode, afterId: preview.result.next_after_id })
      onDeleted()
    }, onError: () => { setPreview(undefined); setStale(true) } })
  }
  const blocked = disabled || mutation.isPending
  const changed = preview && (preview.body.revision !== (revision ?? null) || (preview.body.mode === 'selected' && JSON.stringify([...preview.body.ids].sort()) !== JSON.stringify([...selected].sort())))
  const ids = (items: number[]) => items.map(id => '#' + id).join(', ') || '—'
  return <div className="grid gap-2 text-xs">
    <div className="flex flex-wrap gap-2"><Action disabled={blocked || selected.length === 0} onClick={() => prepare('selected')}>{t(target === 'pools' ? 'proxyPools.deletePools' : 'proxyPools.deleteNodes')}</Action>{poolId !== undefined ? <Action disabled={blocked} onClick={() => prepare('failed')}>{t('proxyPools.clearFailed')}</Action> : null}{unused ? <Action disabled={blocked} onClick={() => prepare('unused')}>{t('proxyPools.clearUnused')}</Action> : null}</div>
    {preview ? <div role="alert" className="grid min-w-0 gap-2 rounded-lg border border-[var(--hairline)] p-3"><p>{t('proxyPools.cleanupPreview', { eligible: preview.result.eligible_ids.length, protected: preview.result.protected_ids.length, skipped: preview.result.skipped_ids.length })}</p><p className="break-words">{t('proxyPools.cleanupEligible')}: {ids(preview.result.eligible_ids)}</p>{preview.result.protected_ids.length ? <p className="break-words">{t('proxyPools.cleanupProtected')}: {ids(preview.result.protected_ids)}</p> : null}{preview.result.skipped_ids.length ? <p className="break-words">{t('proxyPools.cleanupSkipped')}: {ids(preview.result.skipped_ids)}</p> : null}<p className="text-muted-foreground">{t(target === 'pools' ? 'proxyPools.poolCleanupHint' : 'proxyPools.nodeCleanupHint')}</p>{preview.result.more_available ? <p>{t('proxyPools.moreCleanup')}</p> : null}<div className="flex flex-wrap gap-2"><Action disabled={blocked || preview.result.eligible_ids.length === 0 || !!changed} onClick={execute}>{t('proxyPools.confirm')}</Action>{preview.result.eligible_ids.length === 0 && preview.result.next_after_id !== null ? <Action disabled={blocked || !!changed} onClick={() => prepare(preview.body.mode, preview.result.next_after_id!)}>{t('proxyPools.continueCleanup')}</Action> : null}<Action disabled={mutation.isPending} onClick={() => { setPreview(undefined); mutation.reset() }}>{t('proxyPools.cancel')}</Action></div>{!!changed ? <p>{t('proxyPools.cleanupStale')}</p> : null}</div> : null}
    {notice ? <p role="status">{notice}</p> : null}
    {continuation ? <Action disabled={blocked} onClick={() => prepare(continuation.mode, continuation.afterId)}>{t('proxyPools.continueCleanup')}</Action> : null}
    {mutation.isError || stale ? <p role="alert" className="text-destructive">{t(stale ? 'proxyPools.cleanupStale' : 'proxyPools.actionError')}</p> : null}
  </div>
}
function Action({disabled,onClick,children}:{disabled?:boolean;onClick:()=>void;children:ReactNode}) { return <Button size="sm" variant="flat" isDisabled={disabled} onPress={onClick}>{children}</Button> }
