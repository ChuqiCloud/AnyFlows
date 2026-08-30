import { AlertTriangle, Check, ClipboardCheck, Clock3, LoaderCircle, Plus, RefreshCw, ShieldX, X } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import type { OrganizationApprovalRequest, OrganizationApprovalTemplate } from '@/lib/api/generated/types.gen'
import { organizationApprovalErrorCode, organizationApprovalErrorStatus, useCreateOrganizationApprovalRequest, useCreateOrganizationApprovalTemplate, useDecideOrganizationApprovalRequest, useOrganizationApprovalRequests, useOrganizationApprovalTemplates } from './organization-approval-api'

const APPROVALS_READ = 'organization.approvals.read'
const APPROVALS_CREATE = 'organization.approvals.create'
const APPROVALS_MANAGE = 'organization.approvals.manage'
const APPROVALS_DECIDE = 'organization.approvals.decide'
const RESOURCE_TYPES = ['member', 'budget', 'service_account', 'contract_price', 'funds'] as const
type ResourceType = typeof RESOURCE_TYPES[number]

/** 企业审批工作台；状态只来自服务端申请与决定事实投影。 */
export function OrganizationApprovalPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={ClipboardCheck} title={t('organizationApprovals.title')} subtitle={t('organizationApprovals.subtitle')} />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => <OrganizationApprovalWorkspace key={organizationId} organizationId={organizationId} permissions={overview.permissions} />}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationApprovalWorkspace({ organizationId, permissions }: { organizationId: number; permissions: string[] }) {
  const { i18n, t } = useTranslation()
  const canRead = permissions.includes(APPROVALS_READ)
  const canCreate = permissions.includes(APPROVALS_CREATE)
  const canManage = permissions.includes(APPROVALS_MANAGE)
  const canDecide = permissions.includes(APPROVALS_DECIDE)
  const templatesQuery = useOrganizationApprovalTemplates(organizationId, canRead || canCreate)
  const requestsQuery = useOrganizationApprovalRequests(organizationId, canRead || canCreate)
  const createTemplate = useCreateOrganizationApprovalTemplate(organizationId)
  const createRequest = useCreateOrganizationApprovalRequest(organizationId)
  const decideRequest = useDecideOrganizationApprovalRequest(organizationId)
  const [templateOpen, setTemplateOpen] = useState(false)
  const [requestOpen, setRequestOpen] = useState(false)
  const [decisionTarget, setDecisionTarget] = useState<{ request: OrganizationApprovalRequest; kind: 'approved' | 'rejected' }>()
  const templates = templatesQuery.data?.templates ?? []
  const requests = requestsQuery.data?.requests ?? []
  const formatTime = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])
  const error = templatesQuery.error ?? requestsQuery.error ?? createTemplate.error ?? createRequest.error ?? decideRequest.error
  const errorStatus = organizationApprovalErrorStatus(error)
  const errorCode = organizationApprovalErrorCode(error)

  if (!canRead && !canCreate) {
    return <OrganizationPageState icon={ShieldX} title={t('organizationApprovals.restricted.title')} body={t('organizationApprovals.restricted.body')} />
  }
  const refresh = async () => {
    await Promise.all([templatesQuery.refetch(), requestsQuery.refetch()])
    createTemplate.reset(); createRequest.reset(); decideRequest.reset()
  }
  const decide = async () => {
    if (!decisionTarget || decideRequest.isPending) return
    try {
      await decideRequest.mutateAsync({ requestId: decisionTarget.request.id, body: { decision_key: randomKey(), kind: decisionTarget.kind, reason: null, decided_at: null } })
      setDecisionTarget(undefined)
    } catch {
      // 冲突时保留目标，用户刷新后按最新服务端状态再次确认。
    }
  }

  return (
    <div className="grid min-w-0 gap-4">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 border-y border-[var(--hairline)] py-3">
        <div className="flex items-center gap-2 text-xs text-muted-foreground"><ClipboardCheck className="size-3.5 text-brand" aria-hidden="true" /><span>{t('organizationApprovals.summary', { templates: templates.length, requests: requests.length })}</span></div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={templatesQuery.isFetching || requestsQuery.isFetching} onClick={() => void refresh()}><RefreshCw className={templatesQuery.isFetching || requestsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('organizationApprovals.actions.refresh')}</Button>
          <Button type="button" size="sm" variant="secondary" disabled={!canCreate || templates.length === 0} onClick={() => { createRequest.reset(); setRequestOpen(true) }}><Plus aria-hidden="true" />{t('organizationApprovals.actions.createRequest')}</Button>
          <Button type="button" size="sm" disabled={!canManage} onClick={() => { createTemplate.reset(); setTemplateOpen(true) }}><Plus aria-hidden="true" />{t('organizationApprovals.actions.createTemplate')}</Button>
        </div>
      </div>
      {!canManage ? <p className="rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">{t('organizationApprovals.readOnly')}</p> : null}
      {error && !templatesQuery.isError && !requestsQuery.isError ? <p role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(errorStatus === 409 ? 'organizationApprovals.errors.conflict' : errorCode === 'organization_approval_expired' ? 'organizationApprovals.errors.expired' : 'organizationApprovals.errors.mutation')}</p> : null}
      {templatesQuery.isPending || requestsQuery.isPending ? <ApprovalLoading /> : templatesQuery.isError || requestsQuery.isError ? <OrganizationPageState icon={errorStatus === 403 ? ShieldX : AlertTriangle} title={t(errorStatus === 403 ? 'organizationApprovals.errors.forbiddenTitle' : 'organizationApprovals.errors.listTitle')} body={t(errorStatus === 403 ? 'organizationApprovals.errors.forbiddenBody' : 'organizationApprovals.errors.listBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void refresh()}><RefreshCw aria-hidden="true" />{t('organizationApprovals.actions.retry')}</Button>} /> : requests.length === 0 ? <OrganizationPageState icon={ClipboardCheck} title={t('organizationApprovals.empty.title')} body={t('organizationApprovals.empty.body')} /> : <RequestList requests={requests} canDecide={canDecide} formatTime={formatTime} onDecide={(request, kind) => setDecisionTarget({ request, kind })} />}
      <TemplateList templates={templates} formatTime={formatTime} />
      <TemplateForm open={templateOpen} onOpenChange={setTemplateOpen} templates={templates} mutation={createTemplate} />
      <RequestForm open={requestOpen} onOpenChange={setRequestOpen} templates={templates} mutation={createRequest} />
      <DecisionDialog target={decisionTarget} onOpenChange={(open) => { if (!open && !decideRequest.isPending) { decideRequest.reset(); setDecisionTarget(undefined) } }} mutation={decideRequest} onConfirm={() => void decide()} />
    </div>
  )
}

function RequestList({ requests, canDecide, formatTime, onDecide }: { requests: OrganizationApprovalRequest[]; canDecide: boolean; formatTime: Intl.DateTimeFormat; onDecide: (request: OrganizationApprovalRequest, kind: 'approved' | 'rejected') => void }) {
  const { t } = useTranslation()
  return <div className="grid gap-2">{requests.map((request) => <article key={request.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3"><div className="flex min-w-0 flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="font-mono text-xs">#{request.id}</span><Badge>{t(`organizationApprovals.resource.${request.resource_type}`, { defaultValue: request.resource_type })}</Badge><StatusBadge status={request.status} /></div><p className="mt-2 break-words text-sm font-medium">{request.resource_summary}</p></div>{canDecide && request.status === 'pending' ? <div className="flex shrink-0 gap-1"><Button type="button" size="icon-sm" variant="ghost" aria-label={t('organizationApprovals.actions.approve')} onClick={() => onDecide(request, 'approved')}><Check className="text-success" aria-hidden="true" /></Button><Button type="button" size="icon-sm" variant="ghost" aria-label={t('organizationApprovals.actions.reject')} onClick={() => onDecide(request, 'rejected')}><X className="text-destructive" aria-hidden="true" /></Button></div> : null}</div><dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-[0.6875rem] text-muted-foreground sm:grid-cols-3"><div><dt>{t('organizationApprovals.fields.requestedBy')}</dt><dd className="mt-1 font-mono">#{request.requested_by_user_id}</dd></div><div><dt>{t('organizationApprovals.fields.created')}</dt><dd className="mt-1">{formatTime.format(request.created_at * 1000)}</dd></div><div><dt>{t('organizationApprovals.fields.expires')}</dt><dd className="mt-1">{formatTime.format(request.expires_at * 1000)}</dd></div></dl></article>)}</div>
}

function TemplateList({ templates, formatTime }: { templates: OrganizationApprovalTemplate[]; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  if (templates.length === 0) return <p className="text-xs text-muted-foreground">{t('organizationApprovals.templates.empty')}</p>
  return <section className="grid gap-2"><h3 className="text-sm font-semibold">{t('organizationApprovals.templates.title')}</h3><div className="grid gap-2 sm:grid-cols-2">{templates.map((template) => <article key={template.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3"><div className="flex items-center justify-between gap-2"><span className="truncate text-sm font-medium">{template.template_key}</span><Badge>v{template.version}</Badge></div><p className="mt-1 text-xs text-muted-foreground">{t(`organizationApprovals.resource.${template.resource_type}`, { defaultValue: template.resource_type })}</p><p className="mt-2 font-mono text-[0.625rem] text-muted-foreground">{template.definition_digest}</p><p className="mt-2 text-[0.6875rem] text-muted-foreground">{formatTime.format(template.created_at * 1000)}</p></article>)}</div></section>
}

function TemplateForm({ open, onOpenChange, templates, mutation }: { open: boolean; onOpenChange: (open: boolean) => void; templates: OrganizationApprovalTemplate[]; mutation: ReturnType<typeof useCreateOrganizationApprovalTemplate> }) {
  const { t } = useTranslation(); const [key, setKey] = useState(''); const [resource, setResource] = useState<ResourceType>('member'); const [digest, setDigest] = useState(''); const valid = /^[a-f0-9]{64}$/.test(digest) && key.trim() === key && key.length > 0 && key.length <= 64
  const latestVersion = templates.filter((item) => item.template_key === key).reduce((latest, item) => Math.max(latest, item.version), 0)
  const close = (next: boolean) => { onOpenChange(next); if (!next && !mutation.isPending) { setKey(''); setResource('member'); setDigest(''); mutation.reset() } }
  const submit = async (event: FormEvent) => { event.preventDefault(); if (!valid || mutation.isPending) return; try { await mutation.mutateAsync({ template_key: key, resource_type: resource, definition_digest: digest, expected_version: latestVersion || null }); close(false) } catch { /* 保留输入供用户修正 */ } }
  if (!open) return null
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-4"><form className="grid gap-3" onSubmit={submit}><h3 className="text-sm font-semibold">{t('organizationApprovals.forms.templateTitle')}</h3><Input aria-label={t('organizationApprovals.forms.templateKey')} placeholder={t('organizationApprovals.forms.templateKey')} value={key} maxLength={64} disabled={mutation.isPending} onChange={(e) => setKey(e.target.value)} /><Select aria-label={t('organizationApprovals.forms.resourceType')} value={resource} disabled={mutation.isPending} onChange={(e) => setResource(e.target.value as ResourceType)}>{RESOURCE_TYPES.map((item) => <option key={item} value={item}>{t(`organizationApprovals.resource.${item}`)}</option>)}</Select><Input aria-label={t('organizationApprovals.forms.digest')} placeholder={t('organizationApprovals.forms.digest')} value={digest} maxLength={64} disabled={mutation.isPending} onChange={(e) => setDigest(e.target.value)} /><div className="flex justify-end gap-2"><Button type="button" size="sm" variant="ghost" onClick={() => close(false)} disabled={mutation.isPending}>{t('organizationApprovals.actions.cancel')}</Button><Button type="submit" size="sm" disabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}{t('organizationApprovals.actions.create')}</Button></div></form></div>
}

function RequestForm({ open, onOpenChange, templates, mutation }: { open: boolean; onOpenChange: (open: boolean) => void; templates: OrganizationApprovalTemplate[]; mutation: ReturnType<typeof useCreateOrganizationApprovalRequest> }) {
  const { t } = useTranslation(); const [templateId, setTemplateId] = useState(''); const [summary, setSummary] = useState(''); const [resource, setResource] = useState<ResourceType>('member'); const [expires, setExpires] = useState(''); const valid = Boolean(templateId && summary.trim() === summary && summary.length > 0 && summary.length <= 1024 && expires && Number.isSafeInteger(Number(expires)) && Number(expires) > Math.floor(Date.now() / 1000))
  const close = (next: boolean) => { onOpenChange(next); if (!next && !mutation.isPending) { setTemplateId(''); setSummary(''); setResource('member'); setExpires(''); mutation.reset() } }
  const submit = async (event: FormEvent) => { event.preventDefault(); if (!valid || mutation.isPending) return; const template = templates.find((item) => item.id === Number(templateId)); if (!template) return; try { await mutation.mutateAsync({ request_key: randomKey(), resource_type: resource, resource_summary: summary, template_id: template.id, template_version: template.version, expires_at: Number(expires) }); close(false) } catch { /* 保留输入供用户修正 */ } }
  if (!open) return null
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-4"><form className="grid gap-3" onSubmit={submit}><h3 className="text-sm font-semibold">{t('organizationApprovals.forms.requestTitle')}</h3><Select aria-label={t('organizationApprovals.forms.template')} value={templateId} disabled={mutation.isPending} onChange={(e) => setTemplateId(e.target.value)}><option value="">{t('organizationApprovals.forms.templatePlaceholder')}</option>{templates.map((item) => <option key={item.id} value={item.id}>{item.template_key} v{item.version}</option>)}</Select><Select aria-label={t('organizationApprovals.forms.resourceType')} value={resource} disabled={mutation.isPending} onChange={(e) => setResource(e.target.value as ResourceType)}>{RESOURCE_TYPES.map((item) => <option key={item} value={item}>{t(`organizationApprovals.resource.${item}`)}</option>)}</Select><Input aria-label={t('organizationApprovals.forms.summary')} placeholder={t('organizationApprovals.forms.summary')} value={summary} maxLength={1024} disabled={mutation.isPending} onChange={(e) => setSummary(e.target.value)} /><Input aria-label={t('organizationApprovals.forms.expires')} type="datetime-local" value={expires ? new Date(Number(expires) * 1000).toISOString().slice(0, 16) : ''} disabled={mutation.isPending} onChange={(e) => setExpires(String(Math.floor(new Date(e.target.value).getTime() / 1000)))} /><div className="flex justify-end gap-2"><Button type="button" size="sm" variant="ghost" onClick={() => close(false)} disabled={mutation.isPending}>{t('organizationApprovals.actions.cancel')}</Button><Button type="submit" size="sm" disabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}{t('organizationApprovals.actions.submit')}</Button></div></form></div>
}

function DecisionDialog({ target, onOpenChange, mutation, onConfirm }: { target?: { request: OrganizationApprovalRequest; kind: 'approved' | 'rejected' }; onOpenChange: (open: boolean) => void; mutation: ReturnType<typeof useDecideOrganizationApprovalRequest>; onConfirm: () => void }) {
  const { t } = useTranslation(); if (!target) return null
  return <div role="dialog" aria-modal="true" className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-4 shadow-lg"><div className="flex items-center gap-2"><Clock3 className="size-4 text-brand" aria-hidden="true" /><h3 className="text-sm font-semibold">{t(target.kind === 'approved' ? 'organizationApprovals.decision.approveTitle' : 'organizationApprovals.decision.rejectTitle')}</h3></div><p className="mt-2 text-xs text-muted-foreground">{t('organizationApprovals.decision.description', { id: target.request.id })}</p>{mutation.isError ? <p role="alert" className="mt-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(organizationApprovalErrorStatus(mutation.error) === 409 ? 'organizationApprovals.errors.conflict' : 'organizationApprovals.errors.mutation')}</p> : null}<div className="mt-4 flex justify-end gap-2"><Button type="button" size="sm" variant="ghost" onClick={() => onOpenChange(false)} disabled={mutation.isPending}>{t('organizationApprovals.actions.cancel')}</Button><Button type="button" size="sm" variant={target.kind === 'approved' ? 'default' : 'destructive'} onClick={onConfirm} disabled={mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : target.kind === 'approved' ? <Check aria-hidden="true" /> : <X aria-hidden="true" />}{t(target.kind === 'approved' ? 'organizationApprovals.actions.approve' : 'organizationApprovals.actions.reject')}</Button></div></div>
}

function StatusBadge({ status }: { status: string }) { const { t } = useTranslation(); return <Badge className={status === 'approved' ? 'text-success' : status === 'rejected' || status === 'timed_out' ? 'text-destructive' : 'text-warning'}>{t(`organizationApprovals.status.${status}`, { defaultValue: status })}</Badge> }
function ApprovalLoading() { return <div className="grid gap-2" role="status">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-28 rounded-lg" />)}</div> }
function randomKey() { const bytes = new Uint8Array(16); crypto.getRandomValues(bytes); return Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('') }
