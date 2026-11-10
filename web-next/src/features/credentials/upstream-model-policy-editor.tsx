import { useMemo, useState } from 'react'
import { Button, Checkbox, Switch } from '@heroui/react'
import { Plus, Save, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { capabilityError, useUpstreamPolicy, useWriteUpstreamPolicy, type ModelObservation, type ModelPolicyConfig, type UpstreamPolicy } from './key-capability-api'
import { CapabilityError, CapabilityField, CapabilityInput, capabilityInputClass, parseModelNames, validInterval, validModelNames } from './key-capability-fields'

export function UpstreamModelPolicyEditor({ channelId, credentialId, observation }: { channelId: number; credentialId?: number; observation?: ModelObservation }) {
  const { t } = useTranslation()
  const query = useUpstreamPolicy(channelId, credentialId)
  const mutation = useWriteUpstreamPolicy(channelId, credentialId)
  const [generation, setGeneration] = useState(0)
  const save = async (body: ModelPolicyConfig) => { const result = await mutation.mutateAsync(body); setGeneration((value) => value + 1); return result }
  const reload = async () => { mutation.reset(); await query.refetch(); setGeneration((value) => value + 1) }
  return <section className="grid min-w-0 gap-3" aria-label={t(credentialId === undefined ? 'keyCapabilities.channelPolicy' : 'keyCapabilities.keyPolicy')}>
    {query.isPending ? <p className="text-xs text-muted-foreground" role="status">{t('keyCapabilities.loading')}</p> : query.isError ? <CapabilityError onRetry={() => void query.refetch()} /> : query.data ? <ModelPolicyForm key={generation} initial={query.data} observation={observation} channel={credentialId === undefined} pending={mutation.isPending} onSave={save} /> : null}
    {mutation.isError ? <CapabilityError code={capabilityError(mutation.error)} onRetry={capabilityError(mutation.error) === 'conflict' ? () => void reload() : undefined} /> : null}
    {mutation.isSuccess ? <p role="status" className="text-xs text-success">{t('keyCapabilities.saved')}</p> : null}
  </section>
}

function ModelPolicyForm({ initial, observation, channel, pending, onSave }: { initial: ModelPolicyConfig; observation?: ModelObservation; channel: boolean; pending: boolean; onSave: (body: ModelPolicyConfig) => Promise<unknown> }) {
  const { t } = useTranslation()
  const [baseVersion] = useState(initial.version)
  const [mode, setMode] = useState(initial.policy.mode)
  const [models, setModels] = useState(initial.policy.models.join('\n'))
  const [disabled, setDisabled] = useState(initial.policy.disabled_models)
  const [autoAdd, setAutoAdd] = useState(initial.auto_add_discovered)
  const [interval, setInterval] = useState(initial.refresh_interval_secs === null ? '' : String(initial.refresh_interval_secs))
  const [search, setSearch] = useState('')
  const [manual, setManual] = useState('')
  const [selected, setSelected] = useState<string[]>([])
  const [dirty, setDirty] = useState(false)
  const [invalidManual, setInvalidManual] = useState(false)
  const names = useMemo(() => parseModelNames(models), [models])
  const intervalValid = validInterval(interval, 300, 2_592_000)
  const listsValid = validModelNames(names) && validModelNames(disabled) && [...names, ...disabled].reduce((bytes, name) => bytes + new TextEncoder().encode(name).length, 0) <= 256 * 1024
  const items = useMemo(() => {
    const records = new Map((observation?.items ?? []).map((item) => [item.upstream_model, item]))
    for (const name of [...names, ...disabled]) if (!records.has(name)) records.set(name, { upstream_model: name, canonical_models: [], enabled: false, blocked_reason: null, source: 'manual' })
    return [...records.values()].sort((a, b) => a.upstream_model.localeCompare(b.upstream_model))
  }, [observation, names, disabled])
  const visible = items.filter((item) => `${item.upstream_model} ${item.canonical_models.join(' ')}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()))
  const localEnabled = (name: string) => !disabled.includes(name) && (mode === 'inherit' || (mode === 'allowlist' ? names.includes(name) : !names.includes(name)))
  const toggleModels = (rawNames: string[], enabled: boolean) => {
    const targets = new Set(rawNames)
    setDisabled((values) => enabled ? values.filter((name) => !targets.has(name)) : [...new Set([...values, ...rawNames])])
    if (enabled && mode === 'allowlist') setModels([...new Set([...names, ...rawNames])].join('\n'))
    if (enabled && mode === 'denylist') setModels(names.filter((name) => !targets.has(name)).join('\n'))
    setDirty(true)
  }
  const addManual = () => {
    const additions = parseModelNames(manual)
    const next = [...new Set([...names, ...additions])]
    if (!additions.length || !validModelNames(next)) { setInvalidManual(true); return }
    setModels(next.join('\n')); setManual(''); setInvalidManual(false); setDirty(true)
  }
  const save = async () => {
    if (!listsValid || !intervalValid) return
    try { await onSave({ version: baseVersion, policy: { mode, models: names, disabled_models: disabled }, auto_add_discovered: autoAdd, refresh_interval_secs: interval === '' ? null : Number(interval) }); setDirty(false) } catch { /* Keep the complete draft for retry. */ }
  }
  return <div className="grid min-w-0 gap-3">
    <p className="text-xs leading-5 text-muted-foreground">{t(channel ? 'keyCapabilities.channelPolicyHint' : 'keyCapabilities.keyPolicyHint')}</p>
    <div className="grid gap-3 sm:grid-cols-2">
      <CapabilityField label={t('keyCapabilities.policyMode')}><select aria-label={t('keyCapabilities.policyMode')} className={capabilityInputClass} disabled={pending} value={mode} onChange={(event) => { const value = event.target.value as UpstreamPolicy['mode']; setMode(value); if (value !== 'allowlist') setAutoAdd(false); setDirty(true) }}>{(['inherit', 'allowlist', 'denylist'] as const).map((value) => <option key={value} value={value}>{t(`keyCapabilities.mode.${value}`)}</option>)}</select></CapabilityField>
      <CapabilityField label={t('keyCapabilities.exactNames')} hint={t(mode === 'allowlist' ? 'keyCapabilities.emptyAllowlist' : 'keyCapabilities.namesHint')}><textarea aria-label={t('keyCapabilities.exactNames')} rows={3} className={capabilityInputClass} value={models} disabled={pending} onChange={(event) => { setModels(event.target.value); setDirty(true) }} /></CapabilityField>
    </div>
    <div className="flex flex-wrap items-center justify-between gap-3 border-y border-[var(--hairline)] py-3"><Switch size="sm" isSelected={autoAdd} isDisabled={pending || mode !== 'allowlist'} onValueChange={(value) => { setAutoAdd(value); setDirty(true) }}>{t('keyCapabilities.autoAdd')}</Switch><p className="text-[11px] text-muted-foreground">{t('keyCapabilities.autoAddHint')}</p><CapabilityField label={t('keyCapabilities.modelInterval')} hint={t(channel ? 'keyCapabilities.channelModelRefreshHint' : 'keyCapabilities.keyModelRefreshHint')}><div className="flex items-center gap-2"><Switch aria-label={t('keyCapabilities.modelAutoRefresh')} size="sm" isSelected={interval !== ''} isDisabled={pending} onValueChange={(value) => { setInterval(value ? '21600' : ''); setDirty(true) }} /><CapabilityInput aria-label={t('keyCapabilities.modelInterval')} inputMode="numeric" value={interval} disabled={pending || interval === ''} onChange={(event) => { setInterval(event.target.value); setDirty(true) }} /></div></CapabilityField></div>
    <div className="flex flex-wrap items-end gap-2"><div className="min-w-0 flex-1"><CapabilityField label={t('keyCapabilities.manualModel')}><CapabilityInput aria-label={t('keyCapabilities.manualModel')} placeholder={t('keyCapabilities.rawName')} value={manual} disabled={pending} onChange={(event) => setManual(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); addManual() } }} /></CapabilityField></div><Button type="button" size="sm" variant="bordered" isDisabled={pending || !manual.trim()} onPress={addManual}><Plus className="size-3.5" />{t('keyCapabilities.add')}</Button></div>
    {invalidManual || !listsValid || !intervalValid ? <CapabilityError code="invalid" /> : null}
    <div className="flex flex-wrap items-center gap-2"><div className="relative min-w-40 flex-1"><Search className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-muted-foreground" /><input aria-label={t('keyCapabilities.searchModels')} className={`${capabilityInputClass} pl-8`} placeholder={t('keyCapabilities.searchModels')} value={search} onChange={(event) => setSearch(event.target.value)} /></div><Button type="button" size="sm" variant="light" isDisabled={pending || selected.length === 0} onPress={() => toggleModels(selected, true)}>{t('keyCapabilities.enableSelected')}</Button><Button type="button" size="sm" variant="light" isDisabled={pending || selected.length === 0} onPress={() => toggleModels(selected, false)}>{t('keyCapabilities.disableSelected')}</Button></div>
    <div className="max-h-80 overflow-auto border-y border-[var(--hairline)]"><table className="w-full table-fixed text-left text-xs"><thead className="sticky top-0 bg-surface-1 text-[11px] text-muted-foreground"><tr><th className="w-9 p-2"><Checkbox aria-label={t('keyCapabilities.selectModels')} size="sm" isSelected={visible.length > 0 && visible.every((item) => selected.includes(item.upstream_model))} onValueChange={(checked) => setSelected(checked ? visible.map((item) => item.upstream_model) : [])} /></th><th className="p-2">{t('keyCapabilities.model')}</th><th className="w-28 p-2">{t('keyCapabilities.effective')}</th><th className="w-16 p-2">{t('keyCapabilities.local')}</th></tr></thead><tbody>{visible.map((item) => <tr key={item.upstream_model} className="border-t border-[var(--hairline)]"><td className="p-2"><Checkbox aria-label={t('keyCapabilities.selectModel', { model: item.upstream_model })} size="sm" isSelected={selected.includes(item.upstream_model)} onValueChange={(checked) => setSelected((current) => checked ? [...new Set([...current, item.upstream_model])] : current.filter((name) => name !== item.upstream_model))} /></td><td className="min-w-0 p-2"><p className="break-all font-medium">{item.upstream_model}</p><p className="mt-1 break-all text-[11px] text-muted-foreground">{item.canonical_models.join(', ') || t('keyCapabilities.noCanonical')} · {t(`keyCapabilities.source.${item.source}`, { defaultValue: t('keyCapabilities.source.manual') })}</p></td><td className="p-2 text-[11px]"><span className={item.enabled ? 'text-success' : 'text-muted-foreground'}>{t(!observation ? 'keyCapabilities.unknown' : item.enabled ? 'keyCapabilities.allowed' : 'keyCapabilities.blocked')}</span>{item.blocked_reason ? <p className="mt-1 text-muted-foreground">{t(`keyCapabilities.reason.${item.blocked_reason}`, { defaultValue: t('keyCapabilities.blocked') })}</p> : null}</td><td className="p-2"><Switch aria-label={t('keyCapabilities.toggleModel', { model: item.upstream_model })} size="sm" isDisabled={pending} isSelected={localEnabled(item.upstream_model)} onValueChange={(checked) => toggleModels([item.upstream_model], checked)} /></td></tr>)}</tbody></table>{visible.length === 0 ? <p className="p-4 text-center text-xs text-muted-foreground">{t('keyCapabilities.noModels')}</p> : null}</div>
    <p className="text-[11px] leading-5 text-muted-foreground">{t('keyCapabilities.discoveryEvidence')}{channel ? ` ${t('keyCapabilities.channelTableHint')}` : ''}</p>
    <div className="flex flex-wrap items-center justify-between gap-2"><span className="text-xs text-muted-foreground" role="status">{dirty ? t('keyCapabilities.unsaved') : ''}</span><div className="flex flex-wrap gap-2"><Button type="button" size="sm" variant="light" isDisabled={pending} onPress={() => { setMode('inherit'); setModels(''); setDisabled([]); setAutoAdd(false); setDirty(true) }}>{t('keyCapabilities.resetPolicy')}</Button><Button type="button" color="primary" size="sm" isLoading={pending} isDisabled={!dirty || pending || !listsValid || !intervalValid} onPress={() => void save()}><Save className="size-3.5" />{t('keyCapabilities.savePolicy')}</Button></div></div>
  </div>
}
