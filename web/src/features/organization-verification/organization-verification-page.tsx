import { AccountVerificationPage } from '@/features/account-verification/account-verification-page'
import { VerificationSettingsPanel } from '@/features/account-verification/verification-settings-panel'
import { VerificationAttachment } from '@/features/account-verification/verification-attachment'
import { VerificationMaterials, OrganizationVerificationHistory } from '@/features/account-verification/verification-records'
import { Building2, CheckCircle2, ClipboardCheck, Download, FileText, LoaderCircle, Plus, RefreshCw, ShieldCheck, XCircle } from 'lucide-react'
import type { FormEvent } from 'react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import {
  organizationVerificationErrorCode,
  downloadAdminOrganizationVerificationMaterial,
  useAdminOrganizationVerifications,
  useDecideOrganizationVerification,
  useOrganizationVerification,
  useSubmitOrganizationVerification,
  type OrganizationVerificationResponse,
  type OrganizationVerificationStatus,
  type OrganizationVerificationMaterialDraft,
} from './organization-verification-api'

type MaterialDraft = OrganizationVerificationMaterialDraft

const emptyMaterial = (): MaterialDraft => ({ kind: '' })

export function OrganizationVerificationPage({ admin = false }: { admin?: boolean }) {
  const { t } = useTranslation()
  const [view, setView] = useState<'accounts' | 'organizations' | 'settings'>('accounts')
  if (!admin) return <ApplicantVerificationWorkspace />
  return <div className="grid gap-5"><div className="flex gap-2" role="tablist" aria-label={t('verificationCenter.adminTitle')}><Button type="button" role="tab" aria-selected={view === 'accounts'} onClick={() => setView('accounts')}>{t('verificationCenter.accounts')}</Button><Button type="button" role="tab" aria-selected={view === 'organizations'} onClick={() => setView('organizations')}>{t('verificationCenter.organizations')}</Button><Button type="button" role="tab" aria-selected={view === 'settings'} onClick={() => setView('settings')}>认证配置</Button></div>{view === 'accounts' ? <AccountVerificationPage admin /> : view === 'organizations' ? <AdminVerificationWorkspace /> : <VerificationSettingsPanel />}</div>
}

function ApplicantVerificationWorkspace() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return <div className="flex min-w-0 flex-col gap-4"><OrganizationPageHeader icon={ShieldCheck} title={t('organizationVerification.title')} subtitle={t('organizationVerification.subtitle')} /><OrganizationPageBoundary state={state}>{(organizationId, overview) => <ApplicantVerificationCase key={organizationId} organizationId={organizationId} overview={overview} />}</OrganizationPageBoundary></div>
}

function ApplicantVerificationCase({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const query = useOrganizationVerification(organizationId)
  const mutation = useSubmitOrganizationVerification(organizationId)
  const canSubmit = overview.permissions.includes('organization.profile.write')
  const [summary, setSummary] = useState('')
  const [materials, setMaterials] = useState<MaterialDraft[]>([emptyMaterial()])
  const verification = query.data
  const editable = !verification || verification.case.status === 'needs_supplement' || verification.case.status === 'rejected'

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    await mutation.mutateAsync({
      summary: summary.trim(),
      materials: materials.map((material) => ({ kind: material.kind.trim(), file: material.file })),
    })
    setSummary('')
    setMaterials([emptyMaterial()])
  }

  if (query.isPending) return <VerificationLoading />
  if (query.isError) return <VerificationError onRetry={() => void query.refetch()} />

  return <div className="grid gap-4"><VerificationIdentity overview={overview} /><VerificationPurpose /><VerificationProcess />{verification ? <><ApplicantStatus verification={verification} />{canSubmit ? <><VerificationMaterials endpoint={`/api/organizations/${organizationId}/verification/materials`} materials={verification.materials} /><OrganizationVerificationHistory endpoint={`/api/organizations/${organizationId}/verification/history`} /></> : null}</> : <EmptyVerificationState />}{!canSubmit ? <VerificationNotice title={t('organizationVerification.errors.forbidden')} /> : null}{canSubmit && editable ? <VerificationForm summary={summary} setSummary={setSummary} materials={materials} setMaterials={setMaterials} onSubmit={submit} pending={mutation.isPending} hasExisting={verification !== null} /> : null}{mutation.isError ? <ErrorNotice code={organizationVerificationErrorCode(mutation.error)} /> : null}</div>
}

function VerificationIdentity({ overview }: { overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  return <section className="flex flex-wrap items-center justify-between gap-3 border-y border-[var(--hairline)] py-3"><div className="flex min-w-0 items-center gap-3"><span className="grid size-9 place-items-center rounded-lg bg-brand/10 text-brand"><Building2 className="size-4" aria-hidden="true" /></span><div className="min-w-0"><h3 className="truncate text-sm font-semibold">{overview.name}</h3><p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{overview.slug} · #{overview.id}</p></div></div><Badge>{t(`organizationConsole.roles.${overview.role}`)}</Badge></section>
}

function VerificationPurpose() {
  const { t } = useTranslation()
  return <section className="flex items-start gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-4"><ShieldCheck className="mt-0.5 size-4 shrink-0 text-brand" aria-hidden="true" /><div><h3 className="text-sm font-semibold">{t('organizationVerification.purpose.title')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationVerification.purpose.body')}</p></div></section>
}

function VerificationProcess() {
  const { t } = useTranslation()
  const steps = ['submit', 'review', 'record', 'boundary'] as const
  return (
    <section className="rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-4">
      <h3 className="text-sm font-semibold">{t('organizationVerification.process.title')}</h3>
      <ol className="mt-3 grid gap-3 sm:grid-cols-2">
        {steps.map((step, index) => (
          <li key={step} className="flex min-w-0 items-start gap-3">
            <span className="grid size-6 shrink-0 place-items-center rounded-full bg-brand/10 text-xs font-semibold text-brand">{index + 1}</span>
            <div className="min-w-0">
              <h4 className="text-xs font-semibold">{t(`organizationVerification.process.steps.${step}.title`)}</h4>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(`organizationVerification.process.steps.${step}.body`)}</p>
            </div>
          </li>
        ))}
      </ol>
    </section>
  )
}

function ApplicantStatus({ verification }: { verification: OrganizationVerificationResponse }) {
  const { i18n, t } = useTranslation()
  const dateFormat = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])
  const status = verification.case.status
  const messageKey = status === 'approved' ? 'approved' : status === 'needs_supplement' ? 'supplement' : status === 'rejected' ? 'rejected' : 'readOnly'
  return <Card className="overflow-hidden"><CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45"><div className="flex flex-wrap items-center justify-between gap-2"><CardTitle className="text-base">{t('organizationVerification.applicant.reviewReason')}</CardTitle><StatusBadge status={status} /></div><p className="mt-1 text-xs leading-5 text-muted-foreground">{t(`organizationVerification.applicant.${messageKey}`)}</p></CardHeader><CardContent className="grid gap-3 p-4"><p className="whitespace-pre-wrap text-sm leading-6">{verification.case.summary}</p>{verification.case.review_reason ? <div className="rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">{verification.case.review_reason}</div> : null}<div className="flex flex-wrap gap-x-4 gap-y-1 text-[0.6875rem] text-muted-foreground"><span>{t('organizationVerification.applicant.submittedAt')}: {dateFormat.format(verification.case.created_at * 1_000)}</span><span>{t('organizationVerification.applicant.updatedAt')}: {dateFormat.format(verification.case.updated_at * 1_000)}</span><span>{t('organizationVerification.applicant.materialCount', { count: verification.materials.length })}</span></div></CardContent></Card>
}

function VerificationForm({ summary, setSummary, materials, setMaterials, onSubmit, pending, hasExisting }: { summary: string; setSummary: (value: string) => void; materials: MaterialDraft[]; setMaterials: (value: MaterialDraft[]) => void; onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>; pending: boolean; hasExisting: boolean }) {
  const { t } = useTranslation()
  const updateMaterial = (index: number, field: keyof MaterialDraft, value: string | File | undefined) => setMaterials(materials.map((material, current) => current === index ? { ...material, [field]: value } : material))
  return <Card className="overflow-hidden"><CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45"><CardTitle className="text-base">{t('organizationVerification.form.title')}</CardTitle><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationVerification.form.body')}</p></CardHeader><CardContent className="p-4"><form className="grid gap-4" onSubmit={(event) => void onSubmit(event)}><div className="grid gap-1.5"><Label htmlFor="organization-verification-summary">{t('organizationVerification.form.summary')}</Label><Textarea id="organization-verification-summary" required minLength={1} maxLength={512} placeholder={t('organizationVerification.form.summaryPlaceholder')} value={summary} onChange={(event) => setSummary(event.target.value)} /></div><div className="grid gap-3">{materials.map((material, index) => <MaterialFields key={index} index={index} material={material} canRemove={materials.length > 1} onChange={updateMaterial} onRemove={() => setMaterials(materials.filter((_, current) => current !== index))} />)}</div>{materials.length < 20 ? <Button type="button" size="sm" variant="secondary" className="justify-self-start" onClick={() => setMaterials([...materials, emptyMaterial()])}><Plus aria-hidden="true" />{t('organizationVerification.form.addMaterial')}</Button> : null}<Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ClipboardCheck aria-hidden="true" />}{t(hasExisting ? 'organizationVerification.actions.resubmit' : 'organizationVerification.actions.submit')}</Button></form></CardContent></Card>
}

function MaterialFields({ index, material, canRemove, onChange, onRemove }: { index: number; material: MaterialDraft; canRemove: boolean; onChange: (index: number, field: keyof MaterialDraft, value: string | File | undefined) => void; onRemove: () => void }) {
  const { t } = useTranslation()
  const prefix = `organization-verification-material-${index}`
  return <fieldset className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3"><legend className="px-1 text-xs font-medium">{t('organizationVerification.form.material', { index: index + 1 })}</legend><div className="grid gap-3 sm:grid-cols-2"><Field id={`${prefix}-kind`} label={t('organizationVerification.form.kind')} value={material.kind} required onChange={(value) => onChange(index, 'kind', value)} /><label className="grid gap-1.5 text-xs"><span className="font-medium">{t('organizationVerification.form.file')}</span><Input id={`${prefix}-file`} type="file" required={!material.file} accept=".pdf,.png,.jpg,.jpeg,.webp" onChange={(event) => onChange(index, 'file', event.target.files?.[0])} /><span className="text-[0.6875rem] text-muted-foreground">{material.file ? `${material.file.name} · ${Math.ceil(material.file.size / 1024)} KB` : t('organizationVerification.form.fileHint')}</span></label></div>{material.file ? <VerificationAttachment key={`${material.file.name}-${material.file.lastModified}`} name={material.file.name} size={material.file.size} contentType={material.file.type} file={material.file} /> : null}{canRemove ? <Button type="button" size="sm" variant="ghost" className="justify-self-start text-destructive" onClick={onRemove}>{t('organizationVerification.form.removeMaterial')}</Button> : null}</fieldset>
}

function Field({ id, label, value, onChange, required = false, type = 'text', min }: { id: string; label: string; value: string; onChange: (value: string) => void; required?: boolean; type?: string; min?: number }) { return <label className="grid gap-1.5 text-xs"><span className="font-medium">{label}</span><Input id={id} type={type} min={min} required={required} value={value} onChange={(event) => onChange(event.target.value)} /></label> }

function AdminVerificationWorkspace() {
  const { t } = useTranslation()
  const [status, setStatus] = useState<OrganizationVerificationStatus | undefined>()
  const [selected, setSelected] = useState<OrganizationVerificationResponse>()
  const query = useAdminOrganizationVerifications(status)
  const cases = query.data?.pages.flatMap((page) => page.cases) ?? []
  return <div className="grid gap-4"><header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between"><div><div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand"><ShieldCheck className="size-3.5" aria-hidden="true" />{t('organizationVerification.title')}</div><h2 className="text-lg font-semibold">{t('organizationVerification.admin.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('organizationVerification.admin.subtitle')}</p></div><Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('organizationVerification.actions.refresh')}</Button></header><div className="flex min-w-0 gap-1 overflow-x-auto border-y border-[var(--hairline)] py-2">{(['all', 'submitted', 'in_review', 'needs_supplement', 'approved', 'rejected'] as const).map((value) => <Button key={value} type="button" size="sm" variant={(status ?? 'all') === value ? 'default' : 'ghost'} onClick={() => setStatus(value === 'all' ? undefined : value)}>{t(`organizationVerification.admin.filters.${value}`)}</Button>)}</div>{!query.data && query.isPending ? <VerificationLoading /> : query.isError && !query.data ? <VerificationError onRetry={() => void query.refetch()} admin /> : cases.length === 0 ? <OrganizationPageState icon={FileText} title={t(status ? 'organizationVerification.admin.emptyTitle' : 'organizationVerification.admin.initialTitle')} body={t(status ? 'organizationVerification.admin.emptyBody' : 'organizationVerification.admin.initialBody')} /> : <><div className="grid gap-2">{cases.map((item) => <AdminCaseRow key={item.case.id} item={item} onReview={() => setSelected(item)} />)}</div>{query.hasNextPage ? <Button type="button" size="sm" variant="secondary" className="self-center" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('organizationVerification.actions.loadMore')}</Button> : null}</>}{selected ? <AdminDecisionPanel item={selected} onClose={() => setSelected(undefined)} /> : null}</div>
}

function AdminCaseRow({ item, onReview }: { item: OrganizationVerificationResponse; onReview: () => void }) {
  const { t } = useTranslation()
  return <article className="flex min-w-0 flex-col gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1 p-4 sm:flex-row sm:items-start sm:justify-between"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><h3 className="text-sm font-semibold">{t('organizationVerification.admin.case', { id: item.case.id })}</h3><StatusBadge status={item.case.status} /></div><p className="mt-1 text-xs text-muted-foreground">{t('organizationVerification.admin.organization', { id: item.case.organization_id })} · {t('organizationVerification.admin.applicant', { id: item.case.applicant_user_id })} · {t('organizationVerification.admin.version', { version: item.case.version })}</p><p className="mt-2 line-clamp-2 text-xs leading-5 text-muted-foreground">{item.case.summary}</p></div><Button type="button" size="sm" variant="secondary" className="shrink-0" onClick={onReview}><FileText aria-hidden="true" />{t('organizationVerification.admin.review')}</Button></article>
}

function AdminDecisionPanel({ item, onClose }: { item: OrganizationVerificationResponse; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useDecideOrganizationVerification()
  const [reason, setReason] = useState(item.case.review_reason ?? '')
  const [downloadError, setDownloadError] = useState(false)
  const decide = async (decision: 'request_supplement' | 'approve' | 'reject') => {
    await mutation.mutateAsync({ caseId: item.case.id, body: { expected_version: item.case.version, decision, reason: reason.trim() || undefined } })
    onClose()
  }
  const download = async (material: OrganizationVerificationResponse['materials'][number]) => {
    if (!material.content_available) return
    try {
      setDownloadError(false)
      const { blob, response } = await downloadAdminOrganizationVerificationMaterial(item.case.id, material.id)
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = response.headers.get('content-disposition')?.match(/filename="([^"]+)"/)?.[1] ?? material.file_name
      anchor.click()
      URL.revokeObjectURL(url)
    } catch {
      setDownloadError(true)
    }
  }
  return <div className="fixed inset-0 z-50 grid place-items-center bg-black/30 p-4" role="dialog" aria-modal="true" aria-labelledby="organization-verification-decision-title"><Card className="max-h-[90vh] w-full max-w-2xl overflow-y-auto"><CardHeader className="border-b border-[var(--hairline)]"><div className="flex items-start justify-between gap-3"><div><CardTitle id="organization-verification-decision-title" className="text-base">{t('organizationVerification.admin.review')} · {t('organizationVerification.admin.case', { id: item.case.id })}</CardTitle><p className="mt-1 text-xs text-muted-foreground">{t('organizationVerification.admin.organization', { id: item.case.organization_id })} · {t('organizationVerification.admin.applicant', { id: item.case.applicant_user_id })}</p></div><Button type="button" size="icon-sm" variant="ghost" title={t('organizationVerification.admin.close')} onClick={onClose}><XCircle aria-hidden="true" /><span className="sr-only">{t('organizationVerification.admin.close')}</span></Button></div></CardHeader><CardContent className="grid gap-4 p-4"><div className="rounded-lg bg-surface-2/55 p-3 text-sm leading-6 whitespace-pre-wrap">{item.case.summary}</div><div><h4 className="text-xs font-semibold">{t('organizationVerification.admin.materialAccess')}</h4><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('organizationVerification.admin.materialAccessHint')}</p><div className="mt-2 divide-y divide-[var(--hairline)] rounded-lg border border-[var(--hairline)]">{item.materials.map((material) => <div key={material.id} className="grid gap-1 px-3 py-2 text-xs sm:grid-cols-[minmax(0,1fr)_auto_auto]"><span className="truncate font-medium">{material.file_name}</span><span className="text-muted-foreground">{material.kind} · {material.content_type} · {material.size_bytes} B</span>{material.content_available ? <Button type="button" size="icon-sm" variant="ghost" title={t('organizationVerification.admin.download')} onClick={() => void download(material)}><Download aria-hidden="true" /><span className="sr-only">{t('organizationVerification.admin.download')}</span></Button> : <span className="text-muted-foreground">{t('organizationVerification.admin.unavailable')}</span>}</div>)}</div></div><VerificationMaterials endpoint={`/api/admin/organization-verifications/${item.case.id}/materials`} materials={item.materials} /><OrganizationVerificationHistory endpoint={`/api/admin/organization-verifications/${item.case.id}/history`} /><div className="grid gap-1.5"><Label htmlFor="organization-verification-decision-reason">{t('organizationVerification.admin.reason')}</Label><Textarea id="organization-verification-decision-reason" maxLength={512} placeholder={t('organizationVerification.admin.reasonPlaceholder')} value={reason} onChange={(event) => setReason(event.target.value)} /></div>{downloadError ? <ErrorNotice code="organization_verification_unavailable" admin /> : null}{mutation.isError ? <ErrorNotice code={organizationVerificationErrorCode(mutation.error)} admin /> : null}<div className="flex flex-wrap items-center justify-end gap-2"><Button type="button" variant="ghost" onClick={onClose} disabled={mutation.isPending}>{t('organizationVerification.admin.close')}</Button><Button type="button" variant="secondary" disabled={mutation.isPending || !reason.trim() || item.case.status === 'approved'} onClick={() => void decide('request_supplement')}><ClipboardCheck aria-hidden="true" />{t('organizationVerification.admin.requestSupplement')}</Button><Button type="button" variant="secondary" disabled={mutation.isPending || item.case.status === 'approved' || !reason.trim()} onClick={() => void decide('reject')}><XCircle aria-hidden="true" />{t('organizationVerification.admin.reject')}</Button><Button type="button" disabled={mutation.isPending || item.case.status === 'approved'} onClick={() => void decide('approve')}><CheckCircle2 aria-hidden="true" />{t('organizationVerification.admin.approve')}</Button></div></CardContent></Card></div>
}

function StatusBadge({ status }: { status: OrganizationVerificationStatus }) { const { t } = useTranslation(); const tone = status === 'approved' ? 'bg-success/10 text-success' : status === 'rejected' ? 'bg-destructive/10 text-destructive' : status === 'needs_supplement' ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'; return <Badge className={cn(tone)}>{t(`organizationVerification.status.${status}`)}</Badge> }
function EmptyVerificationState() { const { t } = useTranslation(); return <OrganizationPageState icon={FileText} title={t('organizationVerification.applicant.emptyTitle')} body={t('organizationVerification.applicant.emptyBody')} /> }
function VerificationNotice({ title }: { title: string }) { return <div className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning"><XCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" /><span>{title}</span></div> }
function ErrorNotice({ code, admin = false }: { code?: string; admin?: boolean }) { const { t } = useTranslation(); const key = code?.endsWith('conflict') ? 'conflict' : code?.endsWith('forbidden') ? 'forbidden' : code?.endsWith('unavailable') ? 'unavailable' : admin ? 'mutation' : 'mutation'; return <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t(`organizationVerification.errors.${key}`)}</p> }
function VerificationLoading() { const { t } = useTranslation(); return <div className="flex items-center gap-2 border-y border-[var(--hairline)] py-10 text-xs text-muted-foreground" role="status"><RefreshCw className="size-4 animate-spin" aria-hidden="true" />{t('organizationVerification.admin.loading')}</div> }
function VerificationError({ onRetry, admin = false }: { onRetry: () => void; admin?: boolean }) { const { t } = useTranslation(); return <OrganizationPageState icon={XCircle} title={t(admin ? 'organizationVerification.admin.errorTitle' : 'organizationVerification.errors.loadTitle')} body={t(admin ? 'organizationVerification.admin.errorBody' : 'organizationVerification.errors.loadBody')} action={<Button type="button" size="sm" variant="secondary" onClick={onRetry}><RefreshCw aria-hidden="true" />{t('organizationVerification.actions.retry')}</Button>} /> }
