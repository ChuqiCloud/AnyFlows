import { useRef, useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import type { AdminChannel, ClientProfileBindingSchema, ClientProfileReferenceSchema, ClientProfileOpenCodeSettingsSchema } from '@/lib/api/generated/types.gen'
import { adminChannelsQueryKey } from '@/features/channels/channel-api'
import { bindProfile, clientProfilesKey, setOpenCodeEntry, useChannelProfile, useOpenCodeSettings, useProfileCapabilities, useProfileRevisions, useProfiles } from './client-profile-api'
import { compatibleProfile } from './client-profile-policy'
import { isProfileAccessError, useCursorPage, useProfileSearch } from './client-profile-model'
import { ProfileError, ProfileField, ProfileLoading, ProfileMark, ProfilePagination } from './client-profile-page'
import { ProfileButton, ProfileDrawer, ProfileInput, ProfileSelect } from './client-profile-ui'

export function ChannelProfileDrawer({ channel, onClose }: { channel: AdminChannel; onClose: () => void }) {
  const { t } = useTranslation()
  const binding = useChannelProfile(channel.id)
  const settings = useOpenCodeSettings(channel.id, channel.type === 'opencode')
  const error = binding.error ?? settings.error
  if (binding.isPending || (channel.type === 'opencode' && settings.isPending) || (error && (binding.data === undefined || (channel.type === 'opencode' && !settings.data) || isProfileAccessError(error)))) return <ProfileDrawer title={t('clientProfiles.channelSettings')} onClose={onClose}>{error ? <ProfileError error={error} retry={() => { void binding.refetch(); if (channel.type === 'opencode') void settings.refetch() }} /> : <ProfileLoading />}</ProfileDrawer>
  return <ChannelProfileForm channel={channel} binding={binding.data ?? null} settings={settings.data} readError={error} retry={() => { void binding.refetch(); if (channel.type === 'opencode') void settings.refetch() }} onClose={onClose} />
}

function ChannelProfileForm({ channel, binding, settings, readError, retry, onClose }: { channel: AdminChannel; binding: ClientProfileReferenceSchema | null; settings?: ClientProfileOpenCodeSettingsSchema; readError?: unknown; retry: () => void; onClose: () => void }) {
  const { t } = useTranslation()
  const cache = useQueryClient()
  const currentBinding = useRef<HTMLDivElement>(null)
  const [base, setBase] = useState(() => binding)
  const [selection, setSelection] = useState<ClientProfileBindingSchema | undefined>()
  const [profileKey, setProfileKey] = useState('')
  const [entryBase, setEntryBase] = useState(() => settings)
  const [entry, setEntry] = useState(() => settings?.entry ?? 'inference_v1')
  const [saved, setSaved] = useState(false)
  const search = useProfileSearch()
  const page = useCursorPage<string>(search.debounced)
  const revisionsPage = useCursorPage<number>(profileKey)
  const capabilities = useProfileCapabilities()
  const profiles = useProfiles({ enabled: true, channel_type: channel.type, protocol: channel.protocol, search: search.debounced || undefined }, page.cursor)
  const revisions = useProfileRevisions(profileKey, revisionsPage.cursor, !!profileKey)
  const save = useMutation({ mutationFn: () => bindProfile(channel.id, selection!, base?.version ?? 0), onSuccess: async result => { setBase(result); setSelection(undefined); setSaved(true); await cache.invalidateQueries({ queryKey: clientProfilesKey }); currentBinding.current?.focus() } })
  const saveEntry = useMutation({ mutationFn: () => setOpenCodeEntry(channel.id, entryBase!.version, entry), onSuccess: async result => { setEntryBase(result); setEntry(result.entry); setSaved(true); await cache.invalidateQueries({ queryKey: adminChannelsQueryKey }) } })
  const busy = save.isPending || saveEntry.isPending
  const bindingDirty = !!selection && (selection.key !== base?.binding.key || selection.revision !== base?.binding.revision)
  const entryDirty = !!entryBase && entry !== entryBase.entry
  return <ProfileDrawer title={`${t('clientProfiles.channelSettings')} · ${channel.name}`} dirty={bindingDirty || entryDirty} busy={busy} onClose={onClose}>
    {!!readError && <ProfileError error={readError} retry={retry} />}
    <p className="cp-hint">{t('clientProfiles.channelHint')}</p>
    <div ref={currentBinding} tabIndex={-1} className="cp-version cp-current-binding"><strong>{t('clientProfiles.currentBinding')}</strong><p>{base ? `${base.binding.key} · r${base.binding.revision}` : t('clientProfiles.unbound')}</p><p className="cp-hint">{channel.type} · {channel.protocol}</p></div>
    {(channel.client_simulation_profile || channel.client_simulation_body_profile) && <p className="cp-error">{t('clientProfiles.legacyConflict')}</p>}
    <ProfileField label={t('clientProfiles.compatibleProfiles')}><ProfileInput label={t('clientProfiles.search')} value={search.search} maxLength={128} onChange={search.setSearch} placeholder={t('clientProfiles.search')} disabled={busy} /></ProfileField>
    {profiles.isPending ? <ProfileLoading /> : profiles.isError ? <ProfileError error={profiles.error} retry={() => void profiles.refetch()} /> : <>
      <div className="cp-table">{profiles.data.items.map(profile => <div className="cp-row" key={profile.key}><div className="cp-name"><ProfileMark service={profile.service} /><div><strong>{profile.name}</strong><code>{profile.key} · r{profile.latest_revision}</code></div></div><ProfileButton disabled={busy} onClick={() => { setProfileKey(profile.key); setSelection({ key: profile.key, revision: profile.latest_revision }); save.reset(); setSaved(false) }}>{t(profileKey === profile.key ? 'clientProfiles.selected' : 'clientProfiles.select')}</ProfileButton></div>)}{profiles.data.items.length === 0 && <p className="cp-state">{t('clientProfiles.noCompatible')}</p>}</div>
      <ProfilePagination page={page.page} previous={page.page > 1 ? page.previous : undefined} next={profiles.data.next_cursor ? () => page.next(profiles.data.next_cursor!) : undefined} busy={profiles.isFetching || busy} />
    </>}
    {profileKey && <section className="cp-section"><h3>{t('clientProfiles.chooseRevision')}</h3>{revisions.isPending ? <ProfileLoading /> : revisions.isError ? <ProfileError error={revisions.error} retry={() => void revisions.refetch()} /> : <>
      {revisions.data.items.map(snapshot => <div className="cp-version" key={snapshot.binding.revision}><div className="cp-heading"><strong>r{snapshot.binding.revision} · {snapshot.definition.name}</strong><ProfileButton disabled={busy || !compatibleProfile(snapshot.definition, channel)} onClick={() => { setSelection(snapshot.binding); save.reset(); setSaved(false) }}>{t(selection?.revision === snapshot.binding.revision && selection.key === profileKey ? 'clientProfiles.selected' : 'clientProfiles.select')}</ProfileButton></div>{!compatibleProfile(snapshot.definition, channel) && <p className="cp-hint">{t('clientProfiles.incompatibleRevision')}</p>}<p className="cp-hint">{snapshot.definition.protocols.join(' · ')} · {t(`clientProfiles.sessions.${snapshot.definition.session_strategy}`)}</p><p className="cp-hint">{Object.entries(snapshot.definition.identity_headers).map(([name, value]) => `${name}: ${value}`).join(' · ')}</p></div>)}
      <ProfilePagination page={revisionsPage.page} previous={revisionsPage.page > 1 ? revisionsPage.previous : undefined} next={revisions.data.next_cursor ? () => revisionsPage.next(revisions.data.next_cursor!) : undefined} busy={revisions.isFetching || busy} />
    </>}</section>}
    {selection && <div className="cp-version"><strong>{t('clientProfiles.pendingBinding')}</strong><p>{base ? `${base.binding.key} · r${base.binding.revision}` : t('clientProfiles.unbound')} → {selection.key} · r{selection.revision}</p><p className="cp-hint">{t('clientProfiles.bindingWarning')}</p><ProfileButton primary disabled={busy || !bindingDirty} onClick={() => save.mutate()}>{t(busy ? 'clientProfiles.saving' : 'clientProfiles.applyBinding')}</ProfileButton></div>}
    {save.isError && <ProfileError error={save.error} />}
    {entryBase && <section className="cp-section"><h3>OpenCode</h3><p className="cp-hint">{t('clientProfiles.openCodeHint')}</p>{capabilities.isError ? <ProfileError error={capabilities.error} retry={() => void capabilities.refetch()} /> : <ProfileSelect label={t('clientProfiles.entry')} value={entry} disabled={busy || !capabilities.data} options={(capabilities.data?.opencode_entries ?? []).map(value => ({ value, label: value === 'inference_v1' ? 'Inference v1' : 'Zen v1' }))} onChange={value => { setEntry(value as typeof entry); saveEntry.reset(); setSaved(false) }} />}<ProfileButton disabled={busy || !entryDirty} onClick={() => saveEntry.mutate()}>{t('clientProfiles.saveEntry')}</ProfileButton>{saveEntry.isError && <ProfileError error={saveEntry.error} />}</section>}
    {saved && <p className="cp-success" role="status">{t('clientProfiles.saved')}</p>}
  </ProfileDrawer>
}
