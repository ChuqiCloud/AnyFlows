import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Building2, CheckCircle2, FileCheck2, LoaderCircle, RefreshCw, UserRound } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { ApiError } from '@/lib/api'
import { useAccountEligibility, useAccountVerifications, useVerificationDetail, useSubmitAccountVerification,
  useReviewAccountVerification, materialBlob, verificationBase, type VerificationKind } from './account-verification-api'
import { VerificationAttachment } from './verification-attachment'

export function AccountVerificationPage({ admin = false }: { admin?: boolean }) {
  const { t } = useTranslation()
  const [status, setStatus] = useState<number>()
  const [selected, setSelected] = useState<number>()
  const query = useAccountVerifications(admin, status)
  const entries = query.data?.pages.flatMap((p) => p.cases) ?? []
  return <div className="grid min-w-0 gap-5">
    <header className="flex items-start justify-between gap-3"><div><h2 className="flex items-center gap-2 text-xl font-semibold"><FileCheck2 className="size-5 text-brand" />{t(admin ? 'verificationCenter.adminTitle' : 'verificationCenter.title')}</h2><p className="mt-2 text-sm text-muted-foreground">{t(admin ? 'verificationCenter.adminDescription' : 'verificationCenter.description')}</p></div><Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className="size-4" />{t('verificationCenter.refresh')}</Button></header>
    {!admin ? <AccountVerificationForm onSubmitted={setSelected} /> : null}
    <section className="grid gap-3"><div className="flex flex-wrap items-center justify-between gap-2"><h3 className="font-semibold">{t('verificationCenter.history')}</h3><select aria-label={t('verificationCenter.filter')} className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-2 text-sm" value={status ?? ''} onChange={(e) => { setStatus(e.target.value ? Number(e.target.value) : undefined); setSelected(undefined) }}><option value="">{t('verificationCenter.all')}</option>{[1, 3, 4, 5].map((s) => <option key={s} value={s}>{t(`verificationCenter.status.${s}`)}</option>)}</select></div>
      {query.isPending ? <p role="status">{t('verificationCenter.loading')}</p> : query.isError ? <p role="alert" className="text-destructive">{t('verificationCenter.loadError')}</p> : entries.length === 0 ? <p className="rounded-xl border border-dashed border-[var(--hairline)] p-8 text-center text-sm text-muted-foreground">{t('verificationCenter.empty')}</p> : entries.map((item) => <article key={item.id} className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><p className="text-sm font-medium">{item.subject_name} <span className="ml-2 text-xs text-muted-foreground">{t(`verificationCenter.${item.kind}`)} · #{item.id}{admin ? ` · UID ${item.user_id}` : ''}</span></p><p className="mt-1 text-xs text-muted-foreground">{new Date(item.created_at * 1000).toLocaleString()}</p></div><div className="flex items-center gap-3"><span className={item.status === 4 ? 'text-sm text-success' : item.status === 5 ? 'text-sm text-destructive' : 'text-sm text-muted-foreground'}>{t(`verificationCenter.status.${item.status}`)}</span><Button type="button" size="sm" variant="secondary" onClick={() => setSelected(selected === item.id ? undefined : item.id)}>{t('verificationCenter.details')}</Button></div></div>{selected === item.id ? <VerificationDetailPanel key={`${admin}-${item.id}`} id={item.id} admin={admin} /> : null}</article>)}
      {query.hasNextPage ? <Button type="button" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t('verificationCenter.more')}</Button> : null}
    </section>
  </div>
}

function AccountVerificationForm({ onSubmitted }: { onSubmitted: (id: number) => void }) {
  const { t } = useTranslation()
  const eligibility = useAccountEligibility()
  const mutation = useSubmitAccountVerification()
  const [kind, setKind] = useState<VerificationKind>('individual')
  const [name, setName] = useState('')
  const [summary, setSummary] = useState('')
  const [files, setFiles] = useState<File[]>([])
  const [fileError, setFileError] = useState(false)
  const submit = async (event: FormEvent) => {
    event.preventDefault()
    const valid = files.length > 0 && files.length <= 5 && files.every((f) => f.size > 0 && f.size <= 10_000_000 && ['application/pdf', 'image/png', 'image/jpeg', 'image/webp'].includes(f.type)) && files.reduce((sum, f) => sum + f.size, 0) <= 25_000_000
    setFileError(!valid)
    if (!valid) return
    try { const record = await mutation.mutateAsync({ kind, subject_name: name.trim(), summary: summary.trim(), files }); setFiles([]); setName(''); setSummary(''); onSubmitted(record.id) } catch { /* Keep the form for correction. */ }
  }
  return <section className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-5">
    {eligibility.data?.enterprise_verified ? <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-lg bg-success/10 p-3"><p className="flex items-center gap-2 text-sm text-success"><CheckCircle2 className="size-4" />{t('verificationCenter.enterpriseGranted')}</p><a href="#/console/organization-provisioning" className="text-sm font-medium text-brand">{t('verificationCenter.applyWorkspace')}</a></div> : null}
    <div className="mb-4 flex gap-2" role="tablist" aria-label={t('verificationCenter.kind')}>{(['individual', 'enterprise'] as const).map((value) => <Button key={value} type="button" role="tab" aria-selected={kind === value} variant={kind === value ? 'default' : 'secondary'} onClick={() => { setKind(value); mutation.reset() }}>{value === 'individual' ? <UserRound className="size-4" /> : <Building2 className="size-4" />}{t(`verificationCenter.${value}`)}</Button>)}</div>
    <form onSubmit={(event) => void submit(event)} className="grid gap-4">
      <label className="grid gap-2 text-sm"><span>{t(kind === 'enterprise' ? 'verificationCenter.companyName' : 'verificationCenter.personName')}</span><Input aria-label={t('verificationCenter.subjectName')} required maxLength={128} value={name} onChange={(e) => setName(e.target.value)} /></label>
      <label className="grid gap-2 text-sm"><span>{t('verificationCenter.summary')}</span><Textarea aria-label={t('verificationCenter.summary')} required maxLength={512} value={summary} onChange={(e) => setSummary(e.target.value)} /></label>
      <label className="grid gap-2 text-sm"><span>{t('verificationCenter.materials')}</span><Input key={files.length === 0 ? 'empty' : 'files'} type="file" aria-label={t('verificationCenter.materials')} accept=".pdf,.png,.jpg,.jpeg,.webp" multiple onChange={(e) => { setFiles(Array.from(e.target.files ?? [])); setFileError(false) }} /><span className="text-xs text-muted-foreground">{t('verificationCenter.fileHint')}</span></label>
      <div className="grid gap-3 md:grid-cols-2">{files.map((file, index) => <VerificationAttachment key={`${index}-${file.name}-${file.lastModified}`} name={file.name} size={file.size} contentType={file.type} file={file} />)}</div>
      {fileError ? <p role="alert" className="text-sm text-destructive">{t('verificationCenter.fileError')}</p> : null}
      {mutation.isError ? <MutationError error={mutation.error} /> : null}
      <Button type="submit" className="justify-self-start" disabled={mutation.isPending}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <FileCheck2 className="size-4" />}{t('verificationCenter.submit')}</Button>
    </form>
  </section>
}

function VerificationDetailPanel({ id, admin }: { id: number; admin: boolean }) {
  const { t } = useTranslation()
  const query = useVerificationDetail(id, admin)
  const mutation = useReviewAccountVerification()
  const [reason, setReason] = useState('')
  const review = async (status: number) => {
    if (!query.data) return
    try { await mutation.mutateAsync({ id, expected_version: query.data.case.version, status, reason: reason.trim() || undefined }); setReason('') } catch { /* Render the error beside the action. */ }
  }
  if (query.isPending) return <p role="status" className="pt-4">{t('verificationCenter.loading')}</p>
  if (!query.data || query.isError) return <p role="alert" className="pt-4 text-destructive">{t('verificationCenter.loadError')}</p>
  return <div className="mt-4 grid gap-4 border-t border-[var(--hairline)] pt-4"><p className="whitespace-pre-wrap text-sm">{query.data.case.summary}</p>
    {query.data.case.review_reason ? <p className="rounded-lg bg-surface-2 p-3 text-sm">{t('verificationCenter.reviewReason')}: {query.data.case.review_reason}</p> : null}
    <p className="text-xs text-muted-foreground">{t('verificationCenter.updated')}: {new Date(query.data.case.updated_at * 1000).toLocaleString()}</p>
    <div className="grid gap-3 md:grid-cols-2">{query.data.materials.map((m) => <VerificationAttachment key={m.id} name={m.file_name} contentType={m.content_type} size={m.size_bytes} load={(signal) => materialBlob(`${verificationBase(admin)}/${id}/materials/${m.id}`, signal)} />)}</div>
    {admin && query.data.case.status === 1 ? <div className="grid gap-3"><Textarea aria-label={t('verificationCenter.reviewReason')} maxLength={512} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t('verificationCenter.reasonHint')} /><div className="flex flex-wrap gap-2"><Button type="button" disabled={mutation.isPending} onClick={() => void review(4)}>{t('verificationCenter.approve')}</Button><Button type="button" variant="secondary" disabled={mutation.isPending || !reason.trim()} onClick={() => void review(3)}>{t('verificationCenter.supplement')}</Button><Button type="button" variant="secondary" disabled={mutation.isPending || !reason.trim()} onClick={() => void review(5)}>{t('verificationCenter.reject')}</Button></div>{mutation.isError ? <MutationError error={mutation.error} /> : null}</div> : null}
  </div>
}
function MutationError({ error }: { error: unknown }) {
  const { t } = useTranslation()
  return <p role="alert" className="text-sm text-destructive">{t(error instanceof ApiError && error.status === 409 ? 'verificationCenter.conflict' : 'verificationCenter.saveError')}</p>
}
