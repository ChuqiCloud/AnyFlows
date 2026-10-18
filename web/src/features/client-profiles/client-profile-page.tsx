import { useState, type ReactNode } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Copy, Fingerprint, Plus, RefreshCw } from 'lucide-react'
import { findLogoByName } from '@/components/brand/model-logos'
import type { ClientProfileDefinitionSchema, ClientProfileRecordSchema } from '@/lib/api/generated/types.gen'
import {
  clientProfilesKey, toggleProfile, useProfileCapabilities, useProfileDetail,
  useProfileReferences, useProfileRevisions, useProfiles, validateProfile, writeProfile,
} from './client-profile-api'
import { isProfileAccessError, profileErrorKey, profileLogo, useCursorPage, useProfileSearch } from './client-profile-model'
import { ProfileButton, ProfileDrawer, ProfileInput, ProfileSelect } from './client-profile-ui'
import { validProfileDraft } from './client-profile-policy'
import './client-profiles.css'

type Editor = { mode: 'create' | 'copy' | 'edit' | 'view'; key?: string }

export function ProfileMark({ service, icon }: { service: string; icon?: string | null }) {
  const Icon = findLogoByName(icon ?? profileLogo(undefined, service) ?? '')?.Icon ?? Fingerprint
  return <span className="cp-logo"><Icon className="size-5" aria-hidden="true" /></span>
}

export function ProfileField({ label, children }: { label: string; children: ReactNode }) {
  return <div className="cp-field"><span>{label}</span>{children}</div>
}

export function ProfileError({ error, retry }: { error: unknown; retry?: () => void }) {
  const { t } = useTranslation()
  return <div role="alert" className="cp-error"><p>{t(profileErrorKey(error))}</p>{retry && <ProfileButton onClick={retry}>{t('clientProfiles.retry')}</ProfileButton>}</div>
}

export function ProfileLoading() {
  const { t } = useTranslation()
  return <div className="cp-state" role="status" aria-label={t('clientProfiles.loading')}><div className="cp-skeleton" /><div className="cp-skeleton" /><span>{t('clientProfiles.loading')}</span></div>
}

export function ProfilePagination({ page, previous, next, busy }: { page: number; previous?: () => void; next?: () => void; busy?: boolean }) {
  const { t } = useTranslation()
  return <nav className="cp-pagination" aria-label={t('clientProfiles.pagination')}><span>{t('clientProfiles.page', { page })} · {t('clientProfiles.pageSize')}</span><ProfileButton disabled={!previous || busy} onClick={previous}>{t('clientProfiles.previous')}</ProfileButton><ProfileButton disabled={!next || busy} onClick={next}>{t('clientProfiles.next')}</ProfileButton></nav>
}

export function ClientProfilePage() {
  const { t } = useTranslation()
  const cache = useQueryClient()
  const search = useProfileSearch()
  const [enabled, setEnabled] = useState('')
  const [service, setService] = useState('')
  const [editor, setEditor] = useState<Editor>()
  const [confirm, setConfirm] = useState<ClientProfileRecordSchema>()
  const page = useCursorPage<string>(JSON.stringify([search.debounced, enabled, service]))
  const capabilities = useProfileCapabilities()
  const query = useProfiles({ search: search.debounced || undefined, enabled: enabled === '' ? undefined : enabled === 'true', service: service as ClientProfileRecordSchema['service'] || undefined }, page.cursor)
  const toggle = useMutation({ mutationFn: toggleProfile, onSuccess: async () => { setConfirm(undefined); await cache.invalidateQueries({ queryKey: clientProfilesKey }) } })
  const services = Array.from(new Set(capabilities.data?.templates.map(item => item.service) ?? []))
  return <div className="cp-page">
    <header className="cp-heading"><div><h2>{t('clientProfiles.title')}</h2><p>{t('clientProfiles.description')}</p></div><div className="cp-actions"><ProfileButton onClick={() => void query.refetch()} disabled={query.isFetching}><RefreshCw className="size-4" />{t('clientProfiles.refresh')}</ProfileButton><ProfileButton primary disabled={!capabilities.data} onClick={() => setEditor({ mode: 'create' })}><Plus className="size-4" />{t('clientProfiles.create')}</ProfileButton></div></header>
    <div className="cp-toolbar">
      <ProfileInput label={t('clientProfiles.search')} value={search.search} maxLength={128} placeholder={t('clientProfiles.search')} onChange={search.setSearch} />
      <ProfileSelect label={t('clientProfiles.status')} value={enabled} onChange={setEnabled} options={[{ value: '', label: t('clientProfiles.allStatuses') }, { value: 'true', label: t('clientProfiles.enabled') }, { value: 'false', label: t('clientProfiles.disabled') }]} />
      <ProfileSelect label={t('clientProfiles.service')} value={service} onChange={setService} options={[{ value: '', label: t('clientProfiles.allServices') }, ...services.map(value => ({ value, label: t(`clientProfiles.services.${value}`) }))]} />
    </div>
    {capabilities.isError && <ProfileError error={capabilities.error} retry={() => void capabilities.refetch()} />}
    {toggle.isError && <ProfileError error={toggle.error} retry={() => { toggle.reset(); setConfirm(undefined); void query.refetch() }} />}
    {confirm && <div className="cp-error" role="alert"><p>{t(confirm.enabled ? 'clientProfiles.disableConfirm' : 'clientProfiles.enableConfirm', { name: confirm.name })}</p><div className="cp-actions"><ProfileButton disabled={toggle.isPending} onClick={() => { setConfirm(undefined); toggle.reset() }}>{t('clientProfiles.cancel')}</ProfileButton><ProfileButton disabled={toggle.isPending} onClick={() => toggle.mutate(confirm)}>{t('clientProfiles.confirm')}</ProfileButton></div></div>}
    {query.isPending ? <ProfileLoading /> : query.isError ? <ProfileError error={query.error} retry={() => void query.refetch()} /> : <>
      <div className="cp-table">{query.data.items.length === 0 ? <div className="cp-state">{t('clientProfiles.empty')}</div> : query.data.items.map(record => <article className="cp-row" key={record.key}>
        <div className="cp-name"><ProfileMark service={record.service} /><div><strong>{record.name}</strong><code>{record.key}</code><div className="cp-badges"><span>{t(`clientProfiles.services.${record.service}`)}</span><span>r{record.latest_revision}</span>{record.built_in && <span>{t('clientProfiles.builtIn')}</span>}<span className="cp-badge" data-enabled={record.enabled}>{t(record.enabled ? 'clientProfiles.enabled' : 'clientProfiles.disabled')}</span></div></div></div>
        <div className="cp-actions"><ProfileButton onClick={() => setEditor({ mode: record.built_in ? 'view' : 'edit', key: record.key })}>{t(record.built_in ? 'clientProfiles.details' : 'clientProfiles.edit')}</ProfileButton><ProfileButton onClick={() => setEditor({ mode: 'copy', key: record.key })}><Copy className="size-3.5" />{t('clientProfiles.copy')}</ProfileButton><ProfileButton disabled={toggle.isPending} onClick={() => { toggle.reset(); setConfirm(record) }}>{t(record.enabled ? 'clientProfiles.disable' : 'clientProfiles.enable')}</ProfileButton></div>
      </article>)}</div>
      <ProfilePagination page={page.page} previous={page.page > 1 ? page.previous : undefined} next={query.data.next_cursor ? () => page.next(query.data.next_cursor!) : undefined} busy={query.isFetching} />
    </>}
    {editor && capabilities.data && <ProfileEditor key={`${editor.mode}:${editor.key ?? 'new'}`} editor={editor} templates={capabilities.data.templates} headers={capabilities.data.identity_headers} sessions={capabilities.data.session_strategies} onClose={() => setEditor(undefined)} />}
  </div>
}

function ProfileEditor({ editor, templates, headers, sessions, onClose }: { editor: Editor; templates: ClientProfileDefinitionSchema[]; headers: string[]; sessions: ClientProfileDefinitionSchema['session_strategy'][]; onClose: () => void }) {
  const { t } = useTranslation()
  const detail = useProfileDetail(editor.key)
  if (editor.key && (detail.isPending || (detail.isError && (!detail.data || isProfileAccessError(detail.error))))) return <ProfileDrawer title={t('clientProfiles.details')} onClose={onClose}>{detail.isError ? <ProfileError error={detail.error} retry={() => void detail.refetch()} /> : <ProfileLoading />}</ProfileDrawer>
  const template = detail.data?.snapshot.definition ?? templates[0]
  if (!template) return null
  return <ProfileEditorForm editor={editor} initial={template} record={detail.data?.record} readError={detail.error} retry={() => void detail.refetch()} templates={templates} headers={headers} sessions={sessions} onClose={onClose} />
}

function ProfileEditorForm({ editor, initial, record, readError, retry, templates, headers, sessions, onClose }: { editor: Editor; initial: ClientProfileDefinitionSchema; record?: ClientProfileRecordSchema; readError?: unknown; retry: () => void; templates: ClientProfileDefinitionSchema[]; headers: string[]; sessions: ClientProfileDefinitionSchema['session_strategy'][]; onClose: () => void }) {
  const { t } = useTranslation()
  const cache = useQueryClient()
  const creating = editor.mode === 'create' || editor.mode === 'copy'
  const readonly = editor.mode === 'view'
  // 草稿与并发版本在打开时冻结；后台刷新不能覆盖未保存输入。
  const [base] = useState(() => ({ definition: structuredClone(initial), version: creating ? 0 : record!.version }))
  const [key, setKey] = useState(creating ? '' : record!.key)
  const [definition, setDefinition] = useState(() => structuredClone(base.definition))
  const [tab, setTab] = useState('definition')
  const [valid, setValid] = useState(false)
  const dirty = !readonly && (key !== (creating ? '' : record!.key) || JSON.stringify(definition) !== JSON.stringify(base.definition))
  const save = useMutation({ mutationFn: () => writeProfile(key, definition, base.version), onSuccess: async () => { await cache.invalidateQueries({ queryKey: clientProfilesKey }); onClose() } })
  const validate = useMutation({ mutationFn: () => validateProfile(definition, base.version), onSuccess: () => setValid(true) })
  const busy = save.isPending || validate.isPending
  const change = (value: Partial<ClientProfileDefinitionSchema>) => { setDefinition(current => ({ ...current, ...value })); setValid(false); validate.reset(); save.reset() }
  const compatible = templates.filter(template => template.service === definition.service)
  const protocols = Array.from(new Set(compatible.flatMap(template => template.protocols)))
  const credentials = Array.from(new Set(compatible.flatMap(template => template.credential_kinds)))
  return <ProfileDrawer title={t(creating ? 'clientProfiles.create' : readonly ? 'clientProfiles.details' : 'clientProfiles.edit')} dirty={dirty} busy={busy} onClose={onClose}>
    {!!readError && <ProfileError error={readError} retry={retry} />}
    {!creating && <div className="cp-tabs">{['definition', 'revisions', 'references'].map(value => <ProfileButton key={value} onClick={() => setTab(value)}><span aria-current={tab === value ? 'page' : undefined}>{t(`clientProfiles.${value}`)}</span></ProfileButton>)}</div>}
    {tab === 'definition' ? <form className="cp-form" onSubmit={event => { event.preventDefault(); if (!readonly && !busy && validProfileDraft(key, definition)) save.mutate() }}>
      <div className="cp-grid"><ProfileField label={t('clientProfiles.key')}><ProfileInput label={t('clientProfiles.key')} value={key} maxLength={64} disabled={!creating || busy} onChange={setKey} /></ProfileField><ProfileField label={t('clientProfiles.name')}><ProfileInput label={t('clientProfiles.name')} value={definition.name} maxLength={128} disabled={readonly || busy} onChange={name => change({ name })} /></ProfileField></div>
      <ProfileField label={t('clientProfiles.summary')}><ProfileInput label={t('clientProfiles.summary')} value={definition.description} maxLength={2048} disabled={readonly || busy} onChange={description => change({ description })} /></ProfileField>
      <div className="cp-grid"><ProfileField label={t('clientProfiles.service')}><ProfileSelect label={t('clientProfiles.service')} value={definition.service} disabled={readonly || busy} options={Array.from(new Set(templates.map(template => template.service))).map(value => ({ value, label: t(`clientProfiles.services.${value}`) }))} onChange={service => { const template = templates.find(item => item.service === service); if (template) change({ service: template.service, behavior: template.behavior, protocols: [...template.protocols], credential_kinds: [...template.credential_kinds] }) }} /></ProfileField><ProfileField label={t('clientProfiles.behavior')}><ProfileSelect label={t('clientProfiles.behavior')} value={definition.behavior} disabled={readonly || busy} options={Array.from(new Set(compatible.map(item => item.behavior))).map(value => ({ value, label: t(`clientProfiles.behaviors.${value}`) }))} onChange={behavior => change({ behavior: behavior as ClientProfileDefinitionSchema['behavior'] })} /></ProfileField></div>
      <p className="cp-hint">{t('clientProfiles.serviceHint')}</p>
      <ProfileField label={t('clientProfiles.protocols')}><div className="cp-checks">{protocols.map(protocol => <label key={protocol}><input type="checkbox" disabled={readonly || busy} checked={definition.protocols.includes(protocol)} onChange={event => change({ protocols: event.target.checked ? [...definition.protocols, protocol] : definition.protocols.filter(value => value !== protocol) })} />{protocol}</label>)}</div></ProfileField>
      <ProfileField label={t('clientProfiles.credentials')}><div className="cp-checks">{credentials.map(kind => <label key={kind}><input type="checkbox" disabled={readonly || busy} checked={definition.credential_kinds.includes(kind)} onChange={event => change({ credential_kinds: event.target.checked ? [...definition.credential_kinds, kind] : definition.credential_kinds.filter(value => value !== kind) })} />{kind}</label>)}</div></ProfileField>
      <div className="cp-grid"><ProfileField label={t('clientProfiles.session')}><ProfileSelect label={t('clientProfiles.session')} value={definition.session_strategy} disabled={readonly || busy} options={sessions.map(value => ({ value, label: t(`clientProfiles.sessions.${value}`) }))} onChange={session_strategy => change({ session_strategy: session_strategy as ClientProfileDefinitionSchema['session_strategy'] })} /></ProfileField><ProfileField label={t('clientProfiles.icon')}><div className="cp-name"><ProfileMark service={definition.service} icon={definition.icon} /><ProfileInput label={t('clientProfiles.icon')} value={definition.icon ?? ''} maxLength={64} disabled={readonly || busy} placeholder="OpenCode" onChange={icon => change({ icon: icon || null })} /></div></ProfileField></div>
      <p className="cp-hint">{t('clientProfiles.sessionHint')}</p>
      <section className="cp-section"><h3>{t('clientProfiles.headers')}</h3><p className="cp-hint">{t('clientProfiles.headersHint')}</p><div className="cp-headers">{Object.entries(definition.identity_headers).map(([name, value]) => <div key={name} className="cp-header-row"><code>{name}</code><ProfileInput label={name} value={value} maxLength={512} disabled={readonly || busy} onChange={value => change({ identity_headers: { ...definition.identity_headers, [name]: value } })} />{!readonly && <ProfileButton disabled={busy} onClick={() => change({ identity_headers: Object.fromEntries(Object.entries(definition.identity_headers).filter(([key]) => key !== name)) })}>{t('clientProfiles.remove')}</ProfileButton>}</div>)}</div>{!readonly && headers.some(name => !(name in definition.identity_headers)) && <ProfileSelect label={t('clientProfiles.addHeader')} value="" disabled={busy} options={[{ value: '', label: t('clientProfiles.addHeader') }, ...headers.filter(name => !(name in definition.identity_headers)).map(value => ({ value, label: value }))]} onChange={name => { if (name) change({ identity_headers: { ...definition.identity_headers, [name]: '' } }) }} />}</section>
      {readonly && <p className="cp-hint">{t('clientProfiles.protected')}</p>}
      {save.isError && <ProfileError error={save.error} />}{validate.isError && <ProfileError error={validate.error} />}{valid && <p role="status" className="cp-success">{t('clientProfiles.valid')}</p>}
      {!readonly && <div className="cp-actions"><ProfileButton disabled={busy || !validProfileDraft(key, definition)} onClick={() => validate.mutate()}>{t('clientProfiles.validate')}</ProfileButton><ProfileButton primary type="submit" disabled={busy || !validProfileDraft(key, definition)}>{t(busy ? 'clientProfiles.saving' : creating ? 'clientProfiles.save' : 'clientProfiles.publish')}</ProfileButton></div>}
    </form> : record && <ProfileHistory key={tab} profileKey={record.key} tab={tab} />}
  </ProfileDrawer>
}

function ProfileHistory({ profileKey, tab }: { profileKey: string; tab: string }) {
  const { t } = useTranslation()
  const page = useCursorPage<number>(tab)
  const revisions = useProfileRevisions(profileKey, page.cursor, tab === 'revisions')
  const references = useProfileReferences(profileKey, page.cursor, tab === 'references')
  const query = tab === 'revisions' ? revisions : references
  if (query.isPending) return <ProfileLoading />
  if (query.isError) return <ProfileError error={query.error} retry={() => void query.refetch()} />
  return <div className="cp-form">
    {tab === 'revisions' ? revisions.data?.items.map(snapshot => <article className="cp-version" key={snapshot.binding.revision}><strong>r{snapshot.binding.revision} · {snapshot.definition.name}</strong><p className="cp-hint">{snapshot.definition.protocols.join(' · ')} · {t(`clientProfiles.sessions.${snapshot.definition.session_strategy}`)}</p><p className="cp-hint">{snapshot.definition.description}</p><dl>{Object.entries(snapshot.definition.identity_headers).map(([name, value]) => <div key={name} className="cp-hint"><dt>{name}</dt><dd className="break-all">{value}</dd></div>)}</dl></article>) : references.data?.items.map(reference => <article className="cp-version" key={reference.channel_id}><strong>{t('clientProfiles.channelId', { id: reference.channel_id })}</strong><p className="cp-hint">{reference.binding.key} · r{reference.binding.revision}</p></article>)}
    {query.data?.items.length === 0 && <p className="cp-hint">{t('clientProfiles.noRecords')}</p>}
    <ProfilePagination page={page.page} previous={page.page > 1 ? page.previous : undefined} next={query.data?.next_cursor ? () => page.next(query.data!.next_cursor!) : undefined} busy={query.isFetching} />
  </div>
}
