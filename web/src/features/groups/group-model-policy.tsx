import { useEffect, useMemo, useState } from 'react'
import { Check, Plus, RefreshCw, Search, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import type { AdminGroupModelRatioOverride } from '@/lib/api/generated/types.gen'
import { useAdminModelMetadata } from '@/features/model-management/model-management-api'
import { cn } from '@/lib/utils'
import { ratioFromMicros, ratioToMicros } from './group-form-model'
import { useAdminGroupCatalog } from './group-api'
import { useAdminGroupModelPolicy, useUpdateAdminGroupModelPolicy } from './group-model-policy-api'

const MAX_MODELS = 512
const MAX_MODEL_NAME_BYTES = 256
const MAX_RATIO_OVERRIDES = 1024
type RatioDraft = { sourceGroupId: string; model: string; ratio: string }

export function GroupModelPolicyEditor({ groupId }: { groupId: number }) {
  const { t } = useTranslation()
  const policyQuery = useAdminGroupModelPolicy(groupId)
  if (policyQuery.isPending) return <p role="status" className="p-4 text-sm text-muted-foreground">{t('groups.modelPolicy.loading')}</p>
  if (policyQuery.isError) return <div role="alert" className="m-4 grid gap-3 rounded-lg border border-destructive/25 p-4 text-sm text-destructive"><p>{t('groups.modelPolicy.loadError')}</p><Button type="button" size="sm" variant="secondary" onClick={() => void policyQuery.refetch()}>{t('groups.actions.retry')}</Button></div>
  return <GroupModelPolicyForm key={groupId} groupId={groupId} allowedModels={policyQuery.data.allowed_models} ratioOverrides={policyQuery.data.ratio_overrides} />
}

function GroupModelPolicyForm({ groupId, allowedModels, ratioOverrides }: { groupId: number; allowedModels: string[] | null; ratioOverrides: AdminGroupModelRatioOverride[] }) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState((allowedModels ?? []).join('\n'))
  const [search, setSearch] = useState('')
  const [overrides, setOverrides] = useState<RatioDraft[]>(() => ratioOverrides.map(toRatioDraft))
  const modelsQuery = useAdminModelMetadata(100)
  const groupsQuery = useAdminGroupCatalog()
  const mutation = useUpdateAdminGroupModelPolicy(groupId)
  const groups = groupsQuery.data ?? []
  useEffect(() => { setDraft((allowedModels ?? []).join('\n')); setOverrides(ratioOverrides.map(toRatioDraft)) }, [allowedModels, ratioOverrides])

  const selectedModels = useMemo(() => parseModelDraft(draft), [draft])
  const models = useMemo(() => modelsQuery.data?.pages.flatMap((page) => page.models).filter((model) => model.lifecycle !== 'retired') ?? [], [modelsQuery.data])
  const filteredModels = useMemo(() => { const query = search.trim().toLocaleLowerCase(); return query ? models.filter((model) => `${model.model} ${model.display_name} ${model.provider}`.toLocaleLowerCase().includes(query)) : models }, [models, search])
  const validationError = validateModelDraft(draft)
  const ratioValidationError = validateRatioDrafts(overrides)
  const canSave = !mutation.isPending && !validationError && !ratioValidationError
  const toggleModel = (model: string) => { const next = new Set(selectedModels); if (next.has(model)) next.delete(model); else next.add(model); setDraft([...next].sort((left, right) => left.localeCompare(right)).join('\n')); mutation.reset() }
  const save = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canSave) return
    mutation.mutate({ allowed_models: selectedModels.length > 0 ? selectedModels : null, ratio_overrides: overrides.map((item) => ({ source_group_id: Number(item.sourceGroupId), model: item.model.trim() || '*', ratio_micros: ratioToMicros(item.ratio) ?? 0 })) })
  }

  return <form className="flex min-h-0 flex-1 flex-col" onSubmit={save}>
    <div className="flex-1 space-y-4 overflow-y-auto px-4 py-5">
      <div><h3 className="text-sm font-semibold">{t('groups.modelPolicy.title')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.modelPolicy.description')}</p></div>
      <div className="flex flex-wrap items-center gap-2"><Badge className="bg-surface-2 text-muted-foreground">{selectedModels.length === 0 ? t('groups.modelPolicy.unrestricted') : t('groups.modelPolicy.selectedCount', { count: selectedModels.length })}</Badge><Button type="button" size="xs" variant="ghost" disabled={mutation.isPending || selectedModels.length === 0} onClick={() => { setDraft(''); mutation.reset() }}>{t('groups.modelPolicy.clear')}</Button></div>
      <label className="grid gap-1.5" htmlFor="group-model-policy-search"><span className="text-xs font-medium">{t('groups.modelPolicy.catalogLabel')}</span><span className="relative block"><Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" /><Input id="group-model-policy-search" className="pl-8" value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('groups.modelPolicy.searchPlaceholder')} /></span></label>
      {modelsQuery.isPending ? <div className="grid gap-1.5" aria-label={t('groups.modelPolicy.loadingModels')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-10 rounded-lg" />)}</div> : modelsQuery.isError ? <div role="alert" className="rounded-lg border border-warning/20 bg-warning/8 p-3 text-xs text-muted-foreground"><p>{t('groups.modelPolicy.catalogError')}</p><Button type="button" size="xs" variant="ghost" className="mt-1" onClick={() => void modelsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('groups.actions.retry')}</Button></div> : <div className="grid gap-1 rounded-lg border border-[var(--hairline)] p-1.5" role="listbox" aria-label={t('groups.modelPolicy.catalogLabel')} aria-multiselectable="true">{filteredModels.length === 0 ? <p className="p-2 text-xs text-muted-foreground">{t('groups.modelPolicy.noModels')}</p> : filteredModels.map((model) => { const selected = selectedModels.includes(model.model); return <button key={model.id} type="button" role="option" aria-selected={selected} disabled={mutation.isPending || (!selected && selectedModels.length >= MAX_MODELS)} className={cn('flex min-h-9 items-center gap-2 rounded-md px-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-50', selected && 'bg-surface-2')} onClick={() => toggleModel(model.model)}><Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" /><span className="min-w-0 flex-1 truncate font-mono text-xs">{model.model}</span><span className="max-w-32 truncate text-[0.6875rem] text-muted-foreground">{model.display_name}</span></button> })}{modelsQuery.hasNextPage ? <Button type="button" size="xs" variant="ghost" className="justify-center" disabled={modelsQuery.isFetchingNextPage} onClick={() => void modelsQuery.fetchNextPage()}>{t(modelsQuery.isFetchingNextPage ? 'groups.modelPolicy.loadingMore' : 'groups.modelPolicy.loadMore')}</Button> : null}</div>}
      <label className="grid gap-1.5" htmlFor="group-model-policy-custom"><span className="text-xs font-medium">{t('groups.modelPolicy.customLabel')}</span><Textarea id="group-model-policy-custom" value={draft} disabled={mutation.isPending} aria-invalid={validationError !== undefined} onChange={(event) => { setDraft(event.target.value); mutation.reset() }} placeholder={t('groups.modelPolicy.customPlaceholder')} /><span className={cn('text-xs leading-5', validationError ? 'text-destructive' : 'text-muted-foreground')}>{validationError ? t(validationError) : t('groups.modelPolicy.customHint', { count: selectedModels.length, maximum: MAX_MODELS })}</span></label>
      <section className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3" aria-labelledby="group-model-policy-ratio-title"><div><h4 id="group-model-policy-ratio-title" className="text-xs font-semibold">{t('groups.modelPolicy.ratioTitle')}</h4><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.modelPolicy.ratioDescription')}</p></div>{overrides.map((item, index) => <div key={`${index}-${item.sourceGroupId}-${item.model}`} className="grid gap-2 rounded-md bg-surface-2 p-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_7rem_auto] sm:items-end"><label className="grid gap-1"><span className="text-[0.6875rem] font-medium">{t('groups.modelPolicy.ratioSource')}</span><Select value={item.sourceGroupId} disabled={mutation.isPending || groupsQuery.isPending} onChange={(event) => { updateOverride(setOverrides, index, { sourceGroupId: event.target.value }); mutation.reset() }}>{item.sourceGroupId && !groups.some((group) => String(group.id) === item.sourceGroupId) ? <option value={item.sourceGroupId}>{t('groups.modelPolicy.ratioGroupHint', { id: item.sourceGroupId })}</option> : null}{groups.map((group) => <option key={group.id} value={group.id}>{group.display_name} · #{group.id}</option>)}</Select></label><label className="grid gap-1"><span className="text-[0.6875rem] font-medium">{t('groups.modelPolicy.ratioModel')}</span><Input value={item.model === '*' ? '' : item.model} disabled={mutation.isPending} placeholder={t('groups.modelPolicy.ratioModelAll')} onChange={(event) => { updateOverride(setOverrides, index, { model: event.target.value }); mutation.reset() }} /></label><label className="grid gap-1"><span className="text-[0.6875rem] font-medium">{t('groups.modelPolicy.ratioValue')}</span><Input inputMode="decimal" value={item.ratio} disabled={mutation.isPending} onChange={(event) => { updateOverride(setOverrides, index, { ratio: event.target.value }); mutation.reset() }} /></label><Button type="button" size="icon" variant="ghost" disabled={mutation.isPending} aria-label={t('groups.modelPolicy.ratioRemove')} onClick={() => { setOverrides((current) => current.filter((_, currentIndex) => currentIndex !== index)); mutation.reset() }}><Trash2 className="size-4" aria-hidden="true" /></Button></div>)}<Button type="button" size="sm" variant="secondary" disabled={mutation.isPending || groups.length === 0 || overrides.length >= MAX_RATIO_OVERRIDES} onClick={() => { setOverrides((current) => [...current, { sourceGroupId: String(groups[0]?.id ?? ''), model: '*', ratio: '1' }]); mutation.reset() }}><Plus className="size-4" aria-hidden="true" />{t('groups.modelPolicy.ratioAdd')}</Button>{groupsQuery.isError ? <p role="alert" className="text-xs text-destructive">{t('groups.modelPolicy.ratioGroupError')}</p> : null}{ratioValidationError ? <p role="alert" className="text-xs text-destructive">{t(ratioValidationError)}</p> : null}{overrides.length === 0 ? <p className="text-xs text-muted-foreground">{t('groups.modelPolicy.ratioEmpty')}</p> : null}</section>
      {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t('groups.modelPolicy.saveError')}</p> : null}{mutation.isSuccess ? <p role="status" className="text-xs text-success">{t('groups.modelPolicy.saved')}</p> : null}
    </div>
    <div className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] p-4"><p className="text-xs text-muted-foreground">{t('groups.modelPolicy.priorityHint')}</p><Button type="submit" disabled={!canSave}>{t(mutation.isPending ? 'groups.modelPolicy.saving' : 'groups.modelPolicy.save')}</Button></div>
  </form>
}

function toRatioDraft(item: AdminGroupModelRatioOverride): RatioDraft { return { sourceGroupId: String(item.source_group_id), model: item.model, ratio: ratioFromMicros(item.ratio_micros) } }
function updateOverride(setOverrides: React.Dispatch<React.SetStateAction<RatioDraft[]>>, index: number, patch: Partial<RatioDraft>) { setOverrides((current) => current.map((item, currentIndex) => currentIndex === index ? { ...item, ...patch } : item)) }
function validateRatioDrafts(overrides: RatioDraft[]): string | undefined {
  if (overrides.length > MAX_RATIO_OVERRIDES) return 'groups.modelPolicy.ratioTooMany'
  const keys = new Set<string>()
  for (const item of overrides) { const model = item.model.trim() || '*'; const micros = ratioToMicros(item.ratio); if (!/^\d+$/.test(item.sourceGroupId) || Number(item.sourceGroupId) <= 0 || micros === undefined || micros < 0) return 'groups.modelPolicy.ratioInvalid'; if (new TextEncoder().encode(model).length > MAX_MODEL_NAME_BYTES || /\p{Cc}/u.test(model)) return 'groups.modelPolicy.ratioInvalid'; const key = `${item.sourceGroupId}:${model}`; if (keys.has(key)) return 'groups.modelPolicy.ratioDuplicate'; keys.add(key) }
  return undefined
}
function parseModelDraft(draft: string): string[] { return [...new Set(draft.split(/[\r\n,]+/u).map((value) => value.trim()).filter(Boolean))] }
function validateModelDraft(draft: string): string | undefined { const rawModels = draft.split(/[\r\n,]+/u).filter((value) => value.length > 0); const models = parseModelDraft(draft); if (rawModels.some((value) => value.trim() !== value || new TextEncoder().encode(value).length > MAX_MODEL_NAME_BYTES || /\p{Cc}/u.test(value))) return 'groups.modelPolicy.invalidName'; if (models.length > MAX_MODELS) return 'groups.modelPolicy.tooMany'; if (models.reduce((total, model) => total + new TextEncoder().encode(model).length, 0) > 32 * 1024) return 'groups.modelPolicy.tooMuchText'; return undefined }
