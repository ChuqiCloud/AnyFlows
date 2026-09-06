import { Megaphone, Pencil, Plus, RefreshCw, Send, Undo2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import type { Announcement } from '@/lib/api/generated/types.gen'
import {
  useAdminAnnouncements,
  useCreateAdminAnnouncement,
  usePublishAdminAnnouncement,
  useRevokeAdminAnnouncement,
  useUpdateAdminAnnouncement,
} from './announcement-api'

type AnnouncementDraft = {
  audience: 'public' | 'authenticated'
  title_zh: string
  title_en: string
  body_zh: string
  body_en: string
  visible_from: string
  visible_until: string
}

const emptyDraft: AnnouncementDraft = {
  audience: 'public',
  title_zh: '',
  title_en: '',
  body_zh: '',
  body_en: '',
  visible_from: '',
  visible_until: '',
}

function toLocalInput(value: number | null | undefined) {
  if (value == null) return ''
  const date = new Date(value * 1_000)
  const pad = (part: number) => String(part).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function toUnix(value: string) {
  if (!value) return null
  const timestamp = Math.floor(new Date(value).getTime() / 1_000)
  return Number.isSafeInteger(timestamp) ? timestamp : null
}

function draftFromAnnouncement(announcement: Announcement): AnnouncementDraft {
  return {
    audience: announcement.audience,
    title_zh: announcement.title_zh,
    title_en: announcement.title_en,
    body_zh: announcement.body_zh,
    body_en: announcement.body_en,
    visible_from: toLocalInput(announcement.visible_from),
    visible_until: toLocalInput(announcement.visible_until),
  }
}

function statusLabel(status: string, t: (key: string) => string) {
  return status === 'published'
    ? t('announcements.status.published')
    : status === 'revoked'
      ? t('announcements.status.revoked')
      : t('announcements.status.draft')
}

/** 管理员公告工作台：草稿编辑、CAS 发布与撤回均复用服务端版本。 */
export function AnnouncementPage() {
  const { t } = useTranslation()
  const query = useAdminAnnouncements()
  const create = useCreateAdminAnnouncement()
  const update = useUpdateAdminAnnouncement()
  const publish = usePublishAdminAnnouncement()
  const revoke = useRevokeAdminAnnouncement()
  const [selected, setSelected] = useState<Announcement>()
  const [draft, setDraft] = useState<AnnouncementDraft>(emptyDraft)
  const [selectionInitialized, setSelectionInitialized] = useState(false)

  useEffect(() => {
    if (!selectionInitialized && query.data?.entries[0]) {
      setSelected(query.data.entries[0])
      setDraft(draftFromAnnouncement(query.data.entries[0]))
      setSelectionInitialized(true)
    }
  }, [query.data, selectionInitialized])

  const syncMutationResult = useCallback((announcement: Announcement | undefined) => {
    if (!announcement) return
    setSelected(announcement)
    setDraft(draftFromAnnouncement(announcement))
    setSelectionInitialized(true)
  }, [])

  useEffect(() => syncMutationResult(create.data), [create.data, syncMutationResult])
  useEffect(() => syncMutationResult(update.data), [syncMutationResult, update.data])
  useEffect(() => syncMutationResult(publish.data), [publish.data, syncMutationResult])
  useEffect(() => syncMutationResult(revoke.data), [revoke.data, syncMutationResult])

  const entries = query.data?.entries ?? []
  const pending = create.isPending || update.isPending || publish.isPending || revoke.isPending
  const mutationError = create.error || update.error || publish.error || revoke.error
  const select = (announcement: Announcement) => {
    setSelected(announcement)
    setDraft(draftFromAnnouncement(announcement))
  }
  const reset = () => {
    setSelected(undefined)
    setDraft(emptyDraft)
    setSelectionInitialized(true)
  }
  const body = {
    audience: draft.audience,
    title_zh: draft.title_zh,
    title_en: draft.title_en,
    body_zh: draft.body_zh,
    body_en: draft.body_en,
    visible_from: toUnix(draft.visible_from),
    visible_until: toUnix(draft.visible_until),
  }
  const save = () => {
    if (selected) {
      if (selected.status !== 'draft') return
      update.mutate({ id: selected.id, body: { ...body, expected_version: selected.version } })
    } else {
      create.mutate(body)
    }
  }
  const transition = (announcement: Announcement, action: 'publish' | 'revoke') => {
    const mutation = action === 'publish' ? publish : revoke
    mutation.mutate({ id: announcement.id, body: { expected_version: announcement.version } })
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand"><Megaphone className="size-3.5" aria-hidden="true" />{t('announcements.eyebrow')}</div>
          <h2 className="text-lg font-semibold">{t('announcements.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('announcements.subtitle')}</p>
        </div>
        <div className="flex gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('announcements.actions.refresh')}</Button>
          <Button type="button" size="sm" onClick={reset}><Plus aria-hidden="true" />{t('announcements.actions.new')}</Button>
        </div>
      </header>

      {mutationError ? <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4 text-sm"><p className="font-semibold text-destructive">{mutationError === publish.error || mutationError === revoke.error ? t('announcements.errors.transition') : t('announcements.errors.save')}</p></div> : null}
      {query.isPending ? <div className="grid gap-2"><Skeleton className="h-20" /><Skeleton className="h-20" /></div> : query.isError ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4 text-sm"><p className="font-semibold text-destructive">{t('announcements.errors.load')}</p><Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>{t('announcements.actions.retry')}</Button></div>
      ) : (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(22rem,0.9fr)]">
          <Card>
            <CardHeader><CardTitle>{t('announcements.list.title')}</CardTitle><CardDescription>{t('announcements.list.description')}</CardDescription></CardHeader>
            <CardContent className="space-y-2">
              {entries.length === 0 ? <p className="rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center text-sm text-muted-foreground">{t('announcements.list.empty')}</p> : entries.map((announcement) => (
                <button key={announcement.id} type="button" onClick={() => select(announcement)} className={`w-full rounded-lg border p-3 text-left transition-colors focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none ${selected?.id === announcement.id ? 'border-brand/60 bg-brand/5' : 'border-[var(--hairline)] hover:bg-surface-2/50'}`}>
                  <div className="flex items-start justify-between gap-3"><span className="line-clamp-1 text-sm font-medium">{announcement.title_zh}</span><Badge>{statusLabel(announcement.status, t)}</Badge></div>
                  <p className="mt-1 line-clamp-1 text-xs text-muted-foreground">v{announcement.version} · {announcement.title_en}</p>
                </button>
              ))}
            </CardContent>
          </Card>

          <Card>
            <CardHeader><CardTitle>{selected ? t('announcements.editor.edit') : t('announcements.editor.new')}</CardTitle><CardDescription>{t('announcements.editor.description')}</CardDescription></CardHeader>
            <CardContent className="space-y-4">
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.audience')}<select value={draft.audience} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, audience: event.target.value as AnnouncementDraft['audience'] })} className="h-9 rounded-md border border-input bg-background px-3 text-sm"><option value="public">{t('announcements.audience.public')}</option><option value="authenticated">{t('announcements.audience.authenticated')}</option></select></label>
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.titleZh')}<Input value={draft.title_zh} maxLength={160} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, title_zh: event.target.value })} /></label>
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.titleEn')}<Input value={draft.title_en} maxLength={160} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, title_en: event.target.value })} /></label>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.bodyZh')}<Textarea value={draft.body_zh} maxLength={8192} rows={7} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, body_zh: event.target.value })} /></label>
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.bodyEn')}<Textarea value={draft.body_en} maxLength={8192} rows={7} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, body_en: event.target.value })} /></label>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.visibleFrom')}<Input type="datetime-local" value={draft.visible_from} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, visible_from: event.target.value })} /></label>
                <label className="grid gap-1.5 text-xs font-medium">{t('announcements.fields.visibleUntil')}<Input type="datetime-local" value={draft.visible_until} disabled={pending || selected?.status !== 'draft' && Boolean(selected)} onChange={(event) => setDraft({ ...draft, visible_until: event.target.value })} /></label>
              </div>
              <div className="flex flex-wrap justify-end gap-2 border-t border-[var(--hairline)] pt-4">
                {selected?.status === 'draft' || !selected ? <Button type="button" disabled={pending} onClick={save}><Pencil aria-hidden="true" />{selected ? t('announcements.actions.save') : t('announcements.actions.create')}</Button> : null}
                {selected?.status === 'draft' ? <Button type="button" variant="secondary" disabled={pending} onClick={() => transition(selected, 'publish')}><Send aria-hidden="true" />{t('announcements.actions.publish')}</Button> : null}
                {selected?.status === 'published' ? <Button type="button" variant="secondary" disabled={pending} onClick={() => transition(selected, 'revoke')}><Undo2 aria-hidden="true" />{t('announcements.actions.revoke')}</Button> : null}
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  )
}
