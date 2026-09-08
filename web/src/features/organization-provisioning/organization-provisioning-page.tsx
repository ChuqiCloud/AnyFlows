import { ArrowRight, Building2, CheckCircle2, ClipboardCheck, Download, LoaderCircle, Plus, RefreshCw, XCircle } from 'lucide-react'
import type { FormEvent, ReactNode } from 'react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { LucideIcon } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import {
  organizationProvisioningErrorCode,
  organizationProvisioningErrorStatus,
  downloadAdminOrganizationProvisioningMaterial,
  useActiveOrganizationProvisioning,
  useAdminOrganizationProvisioning,
  useAdminOrganizationProvisioningMaterials,
  useCreateOrganizationProvisioning,
  useDecideOrganizationProvisioning,
  useOrganizationProvisioningRequest,
  type OrganizationProvisioningRequest,
  type OrganizationProvisioningMaterialDraft,
  type OrganizationProvisioningMaterial,
  type OrganizationProvisioningStatus,
} from './organization-provisioning-api'

type OrganizationProvisioningPageProps = {
  admin?: boolean
}

type AdminProvisioningView = 'review' | 'apply'
const MAX_MATERIAL_BYTES = 50_000_000
const MAX_TOTAL_MATERIAL_BYTES = 100_000_000
const ALLOWED_MATERIAL_TYPES = new Set(['application/pdf', 'image/png', 'image/jpeg', 'image/webp'])
const emptyMaterial = (): OrganizationProvisioningMaterialDraft => ({ id: createIdempotencyKey(), kind: '' })

/** 企业开通入口；管理员可在审批工作台和自己的申请表单之间切换。 */
export function OrganizationProvisioningPage({ admin = false }: OrganizationProvisioningPageProps) {
  const { t } = useTranslation()
  const [adminView, setAdminView] = useState<AdminProvisioningView>('review')

  if (!admin) return <ApplicantProvisioningWorkspace />

  return (
    <div className="grid gap-4">
      <div
        role="tablist"
        aria-label={t('organizationProvisioning.admin.views.label')}
        className="flex w-fit max-w-full gap-1 overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-2/60 p-1"
      >
        {(['review', 'apply'] as const).map((view) => (
          <Button
            key={view}
            type="button"
            role="tab"
            size="sm"
            variant={adminView === view ? 'default' : 'ghost'}
            aria-selected={adminView === view}
            onClick={() => setAdminView(view)}
          >
            {view === 'review' ? <ClipboardCheck aria-hidden="true" /> : <Building2 aria-hidden="true" />}
            {t(`organizationProvisioning.admin.views.${view}`)}
          </Button>
        ))}
      </div>
      {adminView === 'review' ? <AdminProvisioningWorkspace /> : <ApplicantProvisioningWorkspace />}
    </div>
  )
}

function ApplicantProvisioningWorkspace() {
  const { t } = useTranslation()
  const activeQuery = useActiveOrganizationProvisioning()
  const [requestKey, setRequestKey] = useState(() => readRequestKey())
  const [submittedRequest, setSubmittedRequest] = useState<OrganizationProvisioningRequest>()
  const shouldLoadRequest = requestKey !== undefined && !activeQuery.isPending && activeQuery.data === null && !submittedRequest
  const requestQuery = useOrganizationProvisioningRequest(
    requestKey,
    shouldLoadRequest,
  )
  const createMutation = useCreateOrganizationProvisioning()
  const [form, setForm] = useState(() => ({
    idempotency_key: createIdempotencyKey(),
    organization_name: '',
    organization_slug: '',
    business_reason: '',
    materials: [emptyMaterial()],
  }))
  const [materialError, setMaterialError] = useState(false)
  const request = submittedRequest ?? activeQuery.data ?? requestQuery.data

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const totalBytes = form.materials.reduce((total, material) => total + (material.file?.size ?? 0), 0)
    if (form.materials.some((material) => !material.file || !ALLOWED_MATERIAL_TYPES.has(material.file.type) || material.file.size > MAX_MATERIAL_BYTES) || totalBytes > MAX_TOTAL_MATERIAL_BYTES) {
      setMaterialError(true)
      return
    }
    setMaterialError(false)
    const result = await createMutation.mutateAsync({
      ...form,
      organization_name: form.organization_name.trim(),
      organization_slug: form.organization_slug.trim().toLowerCase(),
      business_reason: form.business_reason.trim(),
      materials: form.materials.map((material) => ({ id: material.id, kind: material.kind.trim(), file: material.file })),
    })
    // 直接使用提交响应渲染状态，避免等待 active 查询切换时把已成功的页面重新置为全屏加载。
    setSubmittedRequest(result)
    setRequestKey(result.request_key)
    storeRequestKey(result.request_key)
    if (result.status === 'rejected') {
      setForm((current) => ({ ...current, idempotency_key: createIdempotencyKey() }))
    }
  }

  if (!submittedRequest && (activeQuery.isPending || (shouldLoadRequest && requestQuery.isPending))) return <ProvisioningLoading />
  if (!submittedRequest && activeQuery.isError && activeQuery.error instanceof Error && organizationProvisioningErrorStatus(activeQuery.error) !== 404) {
    return <ProvisioningState title={t('organizationProvisioning.errors.loadTitle')} body={t('organizationProvisioning.errors.loadBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void activeQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationProvisioning.actions.retry')}</Button>} />
  }
  if (!submittedRequest && requestKey !== undefined && requestQuery.isError) {
    return <ProvisioningState title={t('organizationProvisioning.errors.loadTitle')} body={t('organizationProvisioning.errors.loadBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void requestQuery.refetch()}><RefreshCw aria-hidden="true" />{t('organizationProvisioning.actions.retry')}</Button>} />
  }

  return (
    <div className="grid gap-6">
      <PageHeader icon={Building2} title={t('organizationProvisioning.title')} subtitle={t('organizationProvisioning.subtitle')} />
      <ProvisioningFlow current="apply" />
      {request ? <RequestStatus request={request} /> : null}
      {!request || request.status === 'rejected' ? (
        <section className="max-w-2xl rounded-xl border border-[var(--hairline)] bg-surface-1 p-5">
          <div className="mb-5">
            <h3 className="text-sm font-semibold">{t(request ? 'organizationProvisioning.retry.title' : 'organizationProvisioning.form.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(request ? 'organizationProvisioning.retry.body' : 'organizationProvisioning.form.body')}</p>
          </div>
          <form className="grid gap-4" onSubmit={(event) => void submit(event)}>
            <div className="grid gap-1.5">
              <label htmlFor="organization-provisioning-name" className="text-xs font-medium">{t('organizationProvisioning.form.name')}</label>
              <Input id="organization-provisioning-name" required minLength={1} maxLength={128} value={form.organization_name} onChange={(event) => setForm((current) => ({ ...current, organization_name: event.target.value }))} />
            </div>
            <div className="grid gap-1.5">
              <label htmlFor="organization-provisioning-slug" className="text-xs font-medium">{t('organizationProvisioning.form.slug')}</label>
              <Input id="organization-provisioning-slug" required minLength={1} maxLength={64} pattern="[a-z0-9](?:[a-z0-9-]*[a-z0-9])?" value={form.organization_slug} onChange={(event) => setForm((current) => ({ ...current, organization_slug: event.target.value }))} />
              <p className="text-[0.6875rem] text-muted-foreground">{t('organizationProvisioning.form.slugHint')}</p>
            </div>
            <div className="grid gap-1.5">
              <label htmlFor="organization-provisioning-reason" className="text-xs font-medium">{t('organizationProvisioning.form.reason')}</label>
              <Textarea id="organization-provisioning-reason" required minLength={1} maxLength={2000} value={form.business_reason} onChange={(event) => setForm((current) => ({ ...current, business_reason: event.target.value }))} />
            </div>
            <div className="grid gap-3">
              {form.materials.map((material, index) => <ProvisioningMaterialFields key={material.id} index={index} material={material} canRemove={form.materials.length > 1} onChange={(value) => setForm((current) => ({ ...current, materials: current.materials.map((entry, currentIndex) => currentIndex === index ? value : entry) }))} onRemove={() => setForm((current) => ({ ...current, materials: current.materials.filter((_, currentIndex) => currentIndex !== index) }))} />)}
              {form.materials.length < 20 ? <Button type="button" size="sm" variant="secondary" className="justify-self-start" onClick={() => setForm((current) => ({ ...current, materials: [...current.materials, emptyMaterial()] }))}><Plus aria-hidden="true" />{t('organizationProvisioning.form.addMaterial')}</Button> : null}
            </div>
            {materialError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('organizationProvisioning.errors.materials')}</p> : null}
            {createMutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t(errorKey(createMutation.error))}</p> : null}
            <div className="flex flex-wrap items-center gap-3">
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ClipboardCheck aria-hidden="true" />}
                {t('organizationProvisioning.actions.submit')}
              </Button>
              <span className="text-[0.6875rem] text-muted-foreground">{t('organizationProvisioning.form.idempotent')}</span>
            </div>
          </form>
        </section>
      ) : null}
    </div>
  )
}

function ProvisioningMaterialFields({ index, material, canRemove, onChange, onRemove }: { index: number; material: OrganizationProvisioningMaterialDraft; canRemove: boolean; onChange: (value: OrganizationProvisioningMaterialDraft) => void; onRemove: () => void }) {
  const { t } = useTranslation()
  const prefix = `organization-provisioning-material-${index}`
  return <fieldset className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3"><legend className="px-1 text-xs font-medium">{t('organizationProvisioning.form.material', { index: index + 1 })}</legend><div className="grid gap-3 sm:grid-cols-2"><label className="grid gap-1.5 text-xs"><span className="font-medium">{t('organizationProvisioning.form.materialKind')}</span><Input id={`${prefix}-kind`} required value={material.kind} onChange={(event) => onChange({ ...material, kind: event.target.value })} /></label><label className="grid gap-1.5 text-xs"><span className="font-medium">{t('organizationProvisioning.form.materialFile')}</span><Input id={`${prefix}-file`} type="file" required={!material.file} accept=".pdf,.png,.jpg,.jpeg,.webp" onChange={(event) => onChange({ ...material, file: event.target.files?.[0] })} /><span className="text-[0.6875rem] text-muted-foreground">{material.file ? `${material.file.name} · ${Math.ceil(material.file.size / 1024)} KB` : t('organizationProvisioning.form.materialHint')}</span></label></div>{canRemove ? <Button type="button" size="sm" variant="ghost" className="justify-self-start text-destructive" onClick={onRemove}>{t('organizationProvisioning.form.removeMaterial')}</Button> : null}</fieldset>
}

function AdminProvisioningWorkspace() {
  const { t } = useTranslation()
  const [status, setStatus] = useState<OrganizationProvisioningStatus | undefined>()
  const query = useAdminOrganizationProvisioning(status)
  const [selected, setSelected] = useState<OrganizationProvisioningRequest>()
  const [reason, setReason] = useState('')
  const [seatLimit, setSeatLimit] = useState('10')
  const [teamLimit, setTeamLimit] = useState('5')
  const [keyLimit, setKeyLimit] = useState('10')
  const [retentionDays, setRetentionDays] = useState('365')
  const [teamManagementEnabled, setTeamManagementEnabled] = useState(true)
  const [ssoEnabled, setSsoEnabled] = useState(false)
  const [customRolesEnabled, setCustomRolesEnabled] = useState(false)
  const [validUntil, setValidUntil] = useState(() => datetimeLocalAfterDays(365))
  const [graceUntil, setGraceUntil] = useState('')
  const mutation = useDecideOrganizationProvisioning()

  const requests = query.data?.requests ?? []
  const submitDecision = async (decision: 'approve' | 'reject') => {
    if (!selected) return
    const now = Math.floor(Date.now() / 1000)
    const validUntilSeconds = datetimeLocalToSeconds(validUntil)
    const graceUntilSeconds = graceUntil ? datetimeLocalToSeconds(graceUntil) : undefined
    if (decision === 'approve' && (validUntilSeconds === undefined || validUntilSeconds <= now || (graceUntilSeconds !== undefined && graceUntilSeconds < validUntilSeconds))) return
    const body = decision === 'approve'
      ? {
        expected_version: selected.version,
        decision,
        reason: reason.trim() || undefined,
        entitlement: {
          seat_limit: Number(seatLimit),
          team_limit: Number(teamLimit),
          organization_key_limit: Number(keyLimit),
          team_management_enabled: teamManagementEnabled,
          sso_enabled: ssoEnabled,
          custom_roles_enabled: customRolesEnabled,
          audit_retention_days: Number(retentionDays),
          valid_from: now,
          valid_until: validUntilSeconds as number,
          grace_until: graceUntilSeconds ?? null,
        },
      }
      : { expected_version: selected.version, decision, reason: reason.trim() }
    await mutation.mutateAsync({
      requestKey: selected.request_key,
      body,
    })
    setSelected(undefined)
    setReason('')
  }

  return (
    <div className="grid gap-6">
      <PageHeader icon={ClipboardCheck} title={t('organizationProvisioning.admin.title')} subtitle={t('organizationProvisioning.admin.subtitle')} />
      <ProvisioningFlow current="review" />
      <div className="flex flex-wrap items-center gap-2">
        {(['all', 'pending', 'approved', 'rejected'] as const).map((value) => (
          <Button key={value} type="button" size="sm" variant={(status ?? 'all') === value ? 'default' : 'secondary'} onClick={() => setStatus(value === 'all' ? undefined : value)}>{t(`organizationProvisioning.admin.filters.${value}`)}</Button>
        ))}
        <Button type="button" size="icon-sm" variant="ghost" className="ml-auto" aria-label={t('organizationProvisioning.actions.retry')} onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" /></Button>
      </div>
      {query.isPending ? <ProvisioningLoading /> : query.isError ? <ProvisioningState title={t('organizationProvisioning.errors.loadTitle')} body={t('organizationProvisioning.errors.loadBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationProvisioning.actions.retry')}</Button>} /> : (
        <section className="grid gap-2">
          {requests.length === 0 ? <ProvisioningState title={t('organizationProvisioning.admin.emptyTitle')} body={t('organizationProvisioning.admin.emptyBody')} /> : requests.map((request) => (
            <article key={request.request_key} className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2"><h3 className="text-sm font-semibold">{request.organization_name}</h3><StatusBadge status={request.status} /></div>
                  <p className="mt-1 font-mono text-[0.6875rem] text-muted-foreground">{request.organization_slug} · #{request.id}</p>
                  <p className="mt-2 max-w-3xl text-xs leading-5 text-muted-foreground">{request.business_reason}</p>
                </div>
                {request.status === 'pending' ? <Button type="button" size="sm" variant="secondary" onClick={() => setSelected(request)}>{t('organizationProvisioning.admin.review')}</Button> : null}
              </div>
              {selected?.request_key === request.request_key ? <DecisionPanel request={request} reason={reason} setReason={setReason} seatLimit={seatLimit} setSeatLimit={setSeatLimit} teamLimit={teamLimit} setTeamLimit={setTeamLimit} keyLimit={keyLimit} setKeyLimit={setKeyLimit} retentionDays={retentionDays} setRetentionDays={setRetentionDays} teamManagementEnabled={teamManagementEnabled} setTeamManagementEnabled={setTeamManagementEnabled} ssoEnabled={ssoEnabled} setSsoEnabled={setSsoEnabled} customRolesEnabled={customRolesEnabled} setCustomRolesEnabled={setCustomRolesEnabled} validUntil={validUntil} setValidUntil={setValidUntil} graceUntil={graceUntil} setGraceUntil={setGraceUntil} mutation={mutation} onApprove={() => void submitDecision('approve')} onReject={() => void submitDecision('reject')} onCancel={() => setSelected(undefined)} /> : null}
            </article>
          ))}
        </section>
      )}
    </div>
  )
}

type ProvisioningFlowStep = 'apply' | 'review' | 'setup' | 'operate'

/** 把申请、审批和 Owner 后续动作固定成可扫描的顺序，避免管理员只看到一张审批表。 */
function ProvisioningFlow({ current }: { current: ProvisioningFlowStep }) {
  const { t } = useTranslation()
  const steps: ProvisioningFlowStep[] = ['apply', 'review', 'setup', 'operate']
  const currentIndex = steps.indexOf(current)
  return (
    <ol className="grid gap-2 rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4 sm:grid-cols-4 sm:gap-0">
      {steps.map((step, index) => (
        <li key={step} className="flex min-w-0 items-start gap-2 sm:relative sm:pr-4">
          <span className={index <= currentIndex ? 'grid size-6 shrink-0 place-items-center rounded-full bg-brand text-xs font-semibold text-brand-foreground' : 'grid size-6 shrink-0 place-items-center rounded-full border border-[var(--hairline)] text-xs font-semibold text-muted-foreground'}>{index + 1}</span>
          <div className="min-w-0">
            <p className={index === currentIndex ? 'text-xs font-semibold text-foreground' : 'text-xs font-medium text-muted-foreground'}>{t(`organizationProvisioning.flow.${step}.title`)}</p>
            <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">{t(`organizationProvisioning.flow.${step}.body`)}</p>
          </div>
          {index < steps.length - 1 ? <ArrowRight className="mt-1 ml-auto hidden size-3.5 shrink-0 text-muted-foreground/60 sm:block" aria-hidden="true" /> : null}
        </li>
      ))}
    </ol>
  )
}

type DecisionPanelProps = {
  request: OrganizationProvisioningRequest
  reason: string
  setReason: (value: string) => void
  seatLimit: string
  setSeatLimit: (value: string) => void
  teamLimit: string
  setTeamLimit: (value: string) => void
  keyLimit: string
  setKeyLimit: (value: string) => void
  retentionDays: string
  setRetentionDays: (value: string) => void
  teamManagementEnabled: boolean
  setTeamManagementEnabled: (value: boolean) => void
  ssoEnabled: boolean
  setSsoEnabled: (value: boolean) => void
  customRolesEnabled: boolean
  setCustomRolesEnabled: (value: boolean) => void
  validUntil: string
  setValidUntil: (value: string) => void
  graceUntil: string
  setGraceUntil: (value: string) => void
  mutation: ReturnType<typeof useDecideOrganizationProvisioning>
  onApprove: () => void
  onReject: () => void
  onCancel: () => void
}

/** 审批参数只表达管理员选择；创建企业和授权最终由服务端同一事务处理。 */
function DecisionPanel({
  request,
  reason,
  setReason,
  seatLimit,
  setSeatLimit,
  teamLimit,
  setTeamLimit,
  keyLimit,
  setKeyLimit,
  retentionDays,
  setRetentionDays,
  teamManagementEnabled,
  setTeamManagementEnabled,
  ssoEnabled,
  setSsoEnabled,
  customRolesEnabled,
  setCustomRolesEnabled,
  validUntil,
  setValidUntil,
  graceUntil,
  setGraceUntil,
  mutation,
  onApprove,
  onReject,
  onCancel,
}: DecisionPanelProps) {
  const { t } = useTranslation()
  const materialsQuery = useAdminOrganizationProvisioningMaterials(request.request_key)
  const validUntilSeconds = datetimeLocalToSeconds(validUntil)
  const graceUntilSeconds = graceUntil ? datetimeLocalToSeconds(graceUntil) : undefined
  const validCapacity = [seatLimit, teamLimit, keyLimit, retentionDays].every(isPositiveSafeInteger)
  const canApprove = validCapacity
    && validUntilSeconds !== undefined
    && validUntilSeconds > Math.floor(Date.now() / 1000)
    && (graceUntilSeconds === undefined || graceUntilSeconds >= validUntilSeconds)

  return (
    <div className="mt-4 grid gap-4 border-t border-[var(--hairline)] pt-4">
      <div className="grid gap-2">
        <h4 className="text-xs font-semibold">{t('organizationProvisioning.admin.materials')}</h4>
        {materialsQuery.isPending ? <p className="text-xs text-muted-foreground">{t('organizationProvisioning.admin.materialsLoading')}</p> : materialsQuery.isError ? <p role="alert" className="text-xs text-destructive">{t('organizationProvisioning.admin.materialsError')}</p> : materialsQuery.data?.length ? <div className="divide-y divide-[var(--hairline)] rounded-lg border border-[var(--hairline)]">{materialsQuery.data.map((material) => <ProvisioningMaterialRow key={material.id} requestKey={request.request_key} material={material} />)}</div> : <p className="text-xs text-muted-foreground">{t('organizationProvisioning.admin.materialsEmpty')}</p>}
      </div>
      <div className="grid gap-3 sm:grid-cols-4">
        <NumberField label={t('organizationProvisioning.admin.seats')} value={seatLimit} onChange={setSeatLimit} />
        <NumberField label={t('organizationProvisioning.admin.teams')} value={teamLimit} onChange={setTeamLimit} />
        <NumberField label={t('organizationProvisioning.admin.keys')} value={keyLimit} onChange={setKeyLimit} />
        <NumberField label={t('organizationProvisioning.admin.retention')} value={retentionDays} onChange={setRetentionDays} />
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t('organizationProvisioning.admin.validUntil')}</span>
          <Input type="datetime-local" required value={validUntil} onChange={(event) => setValidUntil(event.target.value)} />
        </label>
        <label className="grid gap-1 text-xs">
          <span className="text-muted-foreground">{t('organizationProvisioning.admin.graceUntil')}</span>
          <Input type="datetime-local" value={graceUntil} onChange={(event) => setGraceUntil(event.target.value)} />
        </label>
      </div>
      <div className="flex flex-wrap gap-x-5 gap-y-2 text-xs">
        <CapabilityToggle checked={teamManagementEnabled} onCheckedChange={setTeamManagementEnabled} label={t('organizationProvisioning.admin.teamManagement')} />
        <CapabilityToggle checked={ssoEnabled} onCheckedChange={setSsoEnabled} label={t('organizationProvisioning.admin.sso')} />
        <CapabilityToggle checked={customRolesEnabled} onCheckedChange={setCustomRolesEnabled} label={t('organizationProvisioning.admin.customRoles')} />
      </div>
      <label className="grid gap-1 text-xs">
        <span className="text-muted-foreground">{t('organizationProvisioning.admin.reason')}</span>
        <Textarea value={reason} maxLength={2000} onChange={(event) => setReason(event.target.value)} />
      </label>
      {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(errorKey(mutation.error))}</p> : null}
      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>{t('organizationProvisioning.actions.cancel')}</Button>
        <Button type="button" size="sm" variant="secondary" disabled={mutation.isPending || reason.trim().length === 0} onClick={onReject}><XCircle aria-hidden="true" />{t('organizationProvisioning.admin.reject')}</Button>
        <Button type="button" size="sm" disabled={mutation.isPending || !canApprove} onClick={onApprove}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CheckCircle2 aria-hidden="true" />}{t('organizationProvisioning.admin.approve')}</Button>
      </div>
      <span className="sr-only">{request.request_key}</span>
    </div>
  )
}

function ProvisioningMaterialRow({ requestKey, material }: { requestKey: string; material: OrganizationProvisioningMaterial }) {
  const { t } = useTranslation()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState(false)
  const download = async () => {
    if (!material.content_available || pending) return
    setPending(true)
    setError(false)
    try {
      const { blob, response } = await downloadAdminOrganizationProvisioningMaterial(requestKey, material.id)
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = response.headers.get('content-disposition')?.match(/filename="([^"]+)"/)?.[1] ?? material.file_name
      anchor.click()
      URL.revokeObjectURL(url)
    } catch {
      setError(true)
    } finally {
      setPending(false)
    }
  }
  return <div className="grid gap-1 px-3 py-2 text-xs sm:grid-cols-[minmax(0,1fr)_auto_auto]"><span className="truncate font-medium">{material.file_name}</span><span className="text-muted-foreground">{material.kind} · {material.content_type} · {material.size_bytes} B</span>{material.content_available ? <Button type="button" size="icon-sm" variant="ghost" title={t('organizationProvisioning.admin.download')} disabled={pending} onClick={() => void download()}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Download aria-hidden="true" />}<span className="sr-only">{t('organizationProvisioning.admin.download')}</span></Button> : <span className="text-muted-foreground">{t('organizationProvisioning.admin.unavailable')}</span>}{error ? <span role="alert" className="text-destructive">{t('organizationProvisioning.admin.materialsError')}</span> : null}</div>
}

function NumberField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return <label className="grid gap-1 text-xs"><span className="text-muted-foreground">{label}</span><Input type="number" min={1} inputMode="numeric" value={value} onChange={(event) => onChange(event.target.value)} /></label>
}

function CapabilityToggle({ checked, onCheckedChange, label }: { checked: boolean; onCheckedChange: (value: boolean) => void; label: string }) {
  return <label className="flex items-center gap-2"><Checkbox checked={checked} onCheckedChange={(value) => onCheckedChange(value === true)} /><span>{label}</span></label>
}

function RequestStatus({ request }: { request: OrganizationProvisioningRequest }) {
  const { t } = useTranslation()
  return <section className="max-w-2xl rounded-xl border border-[var(--hairline)] bg-surface-1 p-5"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="flex items-center gap-2"><h3 className="text-sm font-semibold">{request.organization_name}</h3><StatusBadge status={request.status} /></div><p className="mt-1 font-mono text-[0.6875rem] text-muted-foreground">{request.organization_slug} · {request.request_key}</p></div>{request.replayed ? <Badge className="text-muted-foreground">{t('organizationProvisioning.status.replayed')}</Badge> : null}</div><dl className="mt-4 grid gap-3 border-t border-[var(--hairline)] pt-4 text-xs sm:grid-cols-3"><div><dt className="text-muted-foreground">{t('organizationProvisioning.status.version')}</dt><dd className="mt-1 font-mono">v{request.version}</dd></div><div><dt className="text-muted-foreground">{t('organizationProvisioning.status.updated')}</dt><dd className="mt-1">{formatDate(request.updated_at)}</dd></div>{request.organization_id ? <div><dt className="text-muted-foreground">{t('organizationProvisioning.status.organizationId')}</dt><dd className="mt-1 font-mono">{request.organization_id}</dd></div> : null}</dl>{request.decision_reason ? <p className="mt-4 rounded-lg bg-surface-2/60 px-3 py-2 text-xs leading-5">{request.decision_reason}</p> : null}<p className="mt-4 text-xs leading-5 text-muted-foreground">{t(`organizationProvisioning.status.${request.status}Body`)}</p>{request.status === 'approved' && request.organization_id ? <div className="mt-4"><Button asChild type="button" size="sm"><a href={`#/console/organization-onboarding?organization_id=${request.organization_id}`}><Building2 aria-hidden="true" />{t('organizationProvisioning.status.openOnboarding')}<ArrowRight aria-hidden="true" /></a></Button></div> : null}</section>
}

function StatusBadge({ status }: { status: OrganizationProvisioningStatus }) {
  const { t } = useTranslation()
  return <Badge className={status === 'approved' ? 'bg-success/10 text-success' : status === 'rejected' ? 'bg-destructive/10 text-destructive' : 'bg-warning/10 text-warning'}>{t(`organizationProvisioning.status.${status}`)}</Badge>
}

function PageHeader({ icon: Icon, title, subtitle }: { icon: LucideIcon; title: string; subtitle: string }) {
  const { t } = useTranslation()
  return <header><div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand"><Icon className="size-3.5" aria-hidden="true" />{t('organizationConsole.eyebrow')}</div><h2 className="text-lg font-semibold">{title}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{subtitle}</p></header>
}

function ProvisioningLoading() {
  return <div className="grid min-h-48 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1/35" role="status"><LoaderCircle className="size-5 animate-spin text-muted-foreground" aria-hidden="true" /></div>
}

function ProvisioningState({ title, body, action }: { title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-48 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1/35 p-6 text-center"><div className="max-w-sm"><h3 className="text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}

function errorKey(error: unknown) {
  const code = organizationProvisioningErrorCode(error)
  if (code === 'organization_provisioning_conflict') return 'organizationProvisioning.errors.conflict'
  if (code === 'organization_provisioning_unavailable') return 'organizationProvisioning.errors.unavailable'
  return 'organizationProvisioning.errors.mutation'
}

function createIdempotencyKey() {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

function readRequestKey() {
  try {
    const value = window.sessionStorage.getItem('anyflows.organization-provisioning.request-key')
    return value && /^[a-zA-Z0-9_-]{16,128}$/.test(value) ? value : undefined
  } catch {
    return undefined
  }
}

function storeRequestKey(value: string) {
  try {
    window.sessionStorage.setItem('anyflows.organization-provisioning.request-key', value)
  } catch {
    // 浏览器禁用会话存储时仍保留当前页面内的状态。
  }
}

function formatDate(seconds: number) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(seconds * 1000)
}

function isPositiveSafeInteger(value: string) {
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed > 0
}

function datetimeLocalToSeconds(value: string) {
  const milliseconds = Date.parse(value)
  if (!Number.isFinite(milliseconds)) return undefined
  const seconds = Math.floor(milliseconds / 1_000)
  return Number.isSafeInteger(seconds) ? seconds : undefined
}

function datetimeLocalAfterDays(days: number) {
  const date = new Date()
  date.setDate(date.getDate() + days)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}
