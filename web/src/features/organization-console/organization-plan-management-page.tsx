import { Ban, Building2, Check, Layers3, LoaderCircle, Plus, RefreshCw, Search, ShieldCheck } from 'lucide-react'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import type { OrganizationEntitlementProjection, OrganizationPlan, OrganizationPlanCreateRequest } from '@/lib/api/generated/types.gen'

import {
  useAdminOrganizationPlans,
  useCreateAdminOrganizationPlan,
  useDisableAdminOrganizationPlan,
  usePlatformOrganizationEntitlement,
} from './organization-plan-api'

type Draft = {
  name: string
  description: string
  teamManagement: boolean
  sso: boolean
  seatLimit: string
  teamLimit: string
  keyLimit: string
  auditRetentionDays: string
  validityDays: string
  provider: string
  currency: string
  amountMinor: string
}

const initialDraft: Draft = {
  name: '',
  description: '',
  teamManagement: true,
  sso: false,
  seatLimit: '10',
  teamLimit: '3',
  keyLimit: '10',
  auditRetentionDays: '90',
  validityDays: '365',
  provider: 'stripe',
  currency: 'USD',
  amountMinor: '19900',
}

/** 平台管理员维护企业套餐销售快照；停用是唯一允许的历史状态变化。 */
export function OrganizationPlanManagementPage() {
  const { t } = useTranslation()
  const [creating, setCreating] = useState(false)
  const [selected, setSelected] = useState<OrganizationPlan>()
  const query = useAdminOrganizationPlans()
  const plans = query.data?.pages.flatMap((page) => page.plans) ?? []
  const activeCount = plans.filter((plan) => plan.status === 'active').length

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <Layers3 className="size-3.5" aria-hidden="true" />
            {t('organizationPlans.eyebrow')}
          </div>
          <h2 className="text-lg font-semibold">{t('organizationPlans.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('organizationPlans.subtitle')}</p>
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}>
            <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('organizationPlans.actions.refresh')}
          </Button>
          <Button type="button" size="sm" onClick={() => setCreating((value) => !value)}>
            <Plus aria-hidden="true" />
            {t('organizationPlans.actions.create')}
          </Button>
        </div>
      </header>

      <div className="flex flex-wrap gap-x-4 gap-y-1 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('organizationPlans.summary.loaded', { count: plans.length })}</span>
        <span>{t('organizationPlans.summary.active', { count: activeCount })}</span>
      </div>

      <EntitlementLookup />

      {creating ? <CreatePlanPanel onCreated={() => setCreating(false)} /> : null}

      {query.isPending ? <LoadingState /> : query.isError && query.data === undefined ? (
        <StatePanel title={t('organizationPlans.errors.loadTitle')} body={t('organizationPlans.errors.loadBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationPlans.actions.retry')}</Button>} />
      ) : (
        <PlanTable plans={plans} selected={selected} onSelect={setSelected} />
      )}

      {query.isFetchNextPageError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('organizationPlans.errors.more')}</p> : null}
      {query.hasNextPage ? (
        <Button type="button" variant="secondary" className="self-center" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
          {query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}
          {t('organizationPlans.actions.loadMore')}
        </Button>
      ) : null}

      <DisablePlanDialog plan={selected} onClose={() => setSelected(undefined)} />
    </div>
  )
}

function EntitlementLookup() {
  const { t, i18n } = useTranslation()
  const [organizationId, setOrganizationId] = useState('')
  const [reason, setReason] = useState('')
  const [request, setRequest] = useState<{ organizationId: number; reason: string }>()
  const query = usePlatformOrganizationEntitlement(request)
  const parsedOrganizationId = /^\d+$/.test(organizationId) ? Number(organizationId) : 0
  const reasonBytes = new TextEncoder().encode(reason).length
  const valid = Number.isSafeInteger(parsedOrganizationId)
    && parsedOrganizationId > 0
    && reason.length > 0
    && reason.trim() === reason
    && reasonBytes <= 255
  const submit = () => {
    if (!valid) return
    const next = { organizationId: parsedOrganizationId, reason }
    if (request?.organizationId === next.organizationId && request.reason === next.reason) {
      void query.refetch()
      return
    }
    setRequest(next)
  }
  const formatTime = useMemo(
    () => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }),
    [i18n.resolvedLanguage],
  )

  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex items-start gap-3">
          <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-surface-2 text-brand"><ShieldCheck className="size-4" aria-hidden="true" /></span>
          <div>
            <CardTitle className="text-base">{t('organizationPlans.entitlementLookup.title')}</CardTitle>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationPlans.entitlementLookup.subtitle')}</p>
          </div>
        </div>
      </CardHeader>
      <CardContent className="grid gap-4 p-4">
        <div className="grid gap-3 lg:grid-cols-[minmax(10rem,0.35fr)_minmax(16rem,1fr)_auto] lg:items-end">
          <Field id="organization-entitlement-id" label={t('organizationPlans.entitlementLookup.organizationId')}>
            <Input id="organization-entitlement-id" inputMode="numeric" pattern="[0-9]+" value={organizationId} disabled={query.isFetching} onChange={(event) => setOrganizationId(event.target.value)} />
          </Field>
          <Field id="organization-entitlement-reason" label={t('organizationPlans.entitlementLookup.reason')}>
            <Input id="organization-entitlement-reason" maxLength={255} value={reason} disabled={query.isFetching} onChange={(event) => setReason(event.target.value)} />
          </Field>
          <Button type="button" size="sm" disabled={!valid || query.isFetching} onClick={submit}>
            {query.isFetching ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Search aria-hidden="true" />}
            {t(query.data ? 'organizationPlans.entitlementLookup.refresh' : 'organizationPlans.entitlementLookup.search')}
          </Button>
        </div>
        {query.isError && query.data === undefined ? (
          <p role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">
            {t('organizationPlans.entitlementLookup.error')}
          </p>
        ) : null}
        {query.data ? <AdminEntitlementProjection projection={query.data} formatTime={formatTime} /> : (
          <p className="border-t border-[var(--hairline)] pt-3 text-xs leading-5 text-muted-foreground">{t('organizationPlans.entitlementLookup.empty')}</p>
        )}
      </CardContent>
    </Card>
  )
}

function AdminEntitlementProjection({ projection, formatTime }: { projection: OrganizationEntitlementProjection; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  const facts = [
    [t('organizationPlans.entitlementLookup.source'), t(`organizationPlans.entitlementLookup.sources.${projection.source}`)],
    [t('organizationPlans.entitlementLookup.version'), `v${projection.version}`],
    [t('organizationPlans.entitlementLookup.validFrom'), formatTime.format(projection.valid_from * 1_000)],
    [t('organizationPlans.entitlementLookup.validUntil'), formatTime.format(projection.valid_until * 1_000)],
    [t('organizationPlans.entitlementLookup.updatedAt'), formatTime.format(projection.updated_at * 1_000)],
    [t('organizationPlans.entitlementLookup.appliedOrder'), projection.applied_order_id
      ? `${projection.applied_order_id.slice(0, 12)} · ${projection.plan_id?.slice(0, 8) ?? '-'} · v${projection.plan_version ?? '-'}`
      : t('organizationPlans.entitlementLookup.noAppliedOrder')],
  ]
  return (
    <section className="grid gap-3 border-t border-[var(--hairline)] pt-4" aria-live="polite">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-semibold">{t('organizationPlans.entitlementLookup.result', { id: projection.organization_id })}</span>
        <Badge className={projection.status === 'active' ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>{t(`organizationConsole.entitlement.${projection.status}`)}</Badge>
      </div>
      <dl className="grid gap-x-5 gap-y-3 text-xs sm:grid-cols-2 xl:grid-cols-3">
        {facts.map(([label, value]) => <div key={label} className="min-w-0"><dt className="text-muted-foreground">{label}</dt><dd className="mt-1 truncate font-mono font-medium" title={value}>{value}</dd></div>)}
      </dl>
      <p className="text-xs leading-5 text-muted-foreground">
        {t('organizationPlans.entitlementLookup.capacity', { seats: projection.seat_limit, teams: projection.team_limit, keys: projection.organization_key_limit, days: projection.audit_retention_days })}
      </p>
    </section>
  )
}

function CreatePlanPanel({ onCreated }: { onCreated: () => void }) {
  const { t } = useTranslation()
  const mutation = useCreateAdminOrganizationPlan()
  const [draft, setDraft] = useState<Draft>(initialDraft)
  const [error, setError] = useState(false)
  useEffect(() => {
    if (!mutation.isError) return
    setError(true)
  }, [mutation.isError])

  const update = <K extends keyof Draft>(key: K, value: Draft[K]) => setDraft((current) => ({ ...current, [key]: value }))
  const submit = async () => {
    const request = toRequest(draft)
    if (!request) {
      setError(true)
      return
    }
    setError(false)
    try {
      await mutation.mutateAsync(request)
      setDraft(initialDraft)
      onCreated()
    } catch {
      // 保留管理员草稿，允许根据稳定错误分类修正后重试。
    }
  }

  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <CardTitle className="text-base">{t('organizationPlans.create.title')}</CardTitle>
        <p className="text-xs leading-5 text-muted-foreground">{t('organizationPlans.create.subtitle')}</p>
      </CardHeader>
      <CardContent className="grid gap-4 p-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field id="organization-plan-name" label={t('organizationPlans.fields.name')}>
            <Input id="organization-plan-name" maxLength={80} value={draft.name} disabled={mutation.isPending} onChange={(event) => update('name', event.target.value)} />
          </Field>
          <Field id="organization-plan-provider" label={t('organizationPlans.fields.provider')}>
            <Select id="organization-plan-provider" value={draft.provider} disabled={mutation.isPending} onChange={(event) => update('provider', event.target.value)}>
              <option value="stripe">Stripe</option>
              <option value="epay">EasyPay</option>
            </Select>
          </Field>
        </div>
        <Field id="organization-plan-description" label={t('organizationPlans.fields.description')}>
          <Input id="organization-plan-description" maxLength={240} value={draft.description} disabled={mutation.isPending} onChange={(event) => update('description', event.target.value)} />
        </Field>
        <div className="grid gap-3 sm:grid-cols-3">
          <NumberField id="organization-plan-seats" label={t('organizationPlans.fields.seats')} value={draft.seatLimit} onChange={(value) => update('seatLimit', value)} disabled={mutation.isPending} />
          <NumberField id="organization-plan-teams" label={t('organizationPlans.fields.teams')} value={draft.teamLimit} onChange={(value) => update('teamLimit', value)} disabled={mutation.isPending} />
          <NumberField id="organization-plan-keys" label={t('organizationPlans.fields.keys')} value={draft.keyLimit} onChange={(value) => update('keyLimit', value)} disabled={mutation.isPending} />
          <NumberField id="organization-plan-retention" label={t('organizationPlans.fields.auditRetention')} value={draft.auditRetentionDays} onChange={(value) => update('auditRetentionDays', value)} disabled={mutation.isPending} />
          <NumberField id="organization-plan-validity" label={t('organizationPlans.fields.validity')} value={draft.validityDays} onChange={(value) => update('validityDays', value)} disabled={mutation.isPending} />
          <NumberField id="organization-plan-amount" label={t('organizationPlans.fields.amountMinor')} value={draft.amountMinor} onChange={(value) => update('amountMinor', value)} disabled={mutation.isPending} />
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field id="organization-plan-currency" label={t('organizationPlans.fields.currency')}>
            <Input id="organization-plan-currency" maxLength={3} value={draft.currency} disabled={mutation.isPending} onChange={(event) => update('currency', event.target.value.toUpperCase())} />
          </Field>
          <div className="grid gap-2">
            <span className="text-xs font-medium">{t('organizationPlans.fields.capabilities')}</span>
            <label className="flex items-center gap-2 text-xs"><Checkbox checked={draft.teamManagement} disabled={mutation.isPending} onCheckedChange={(value) => update('teamManagement', value === true)} /><span>{t('organizationPlans.fields.teamManagement')}</span></label>
            <label className="flex items-center gap-2 text-xs"><Checkbox checked={draft.sso} disabled={mutation.isPending} onCheckedChange={(value) => update('sso', value === true)} /><span>{t('organizationPlans.fields.sso')}</span></label>
            <label className="flex items-center gap-2 text-xs text-muted-foreground"><Checkbox checked={false} disabled /><span>{t('organizationPlans.fields.customRolesUnavailable')}</span></label>
          </div>
        </div>
        {error ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('organizationPlans.errors.create')}</p> : null}
        <div className="flex justify-end gap-2 border-t border-[var(--hairline)] pt-3">
          <Button type="button" disabled={mutation.isPending} onClick={() => void submit()}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Check aria-hidden="true" />}
            {t(mutation.isPending ? 'organizationPlans.actions.creating' : 'organizationPlans.actions.save')}
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}

function PlanTable({ plans, selected, onSelect }: { plans: OrganizationPlan[]; selected?: OrganizationPlan; onSelect: (plan: OrganizationPlan | undefined) => void }) {
  const { t, i18n } = useTranslation()
  const numberFormat = useMemo(() => new Intl.NumberFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'), [i18n.resolvedLanguage])
  if (plans.length === 0) return <StatePanel title={t('organizationPlans.empty.title')} body={t('organizationPlans.empty.body')} />
  return (
    <div className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-1">
      <table className="w-full min-w-[760px] text-left text-xs">
        <thead className="bg-surface-2/60 text-muted-foreground"><tr><th className="px-3 py-2 font-medium">{t('organizationPlans.columns.plan')}</th><th className="px-3 py-2 font-medium">{t('organizationPlans.columns.capacity')}</th><th className="px-3 py-2 font-medium">{t('organizationPlans.columns.price')}</th><th className="px-3 py-2 font-medium">{t('organizationPlans.columns.validity')}</th><th className="px-3 py-2"><span className="sr-only">{t('organizationPlans.columns.actions')}</span></th></tr></thead>
        <tbody className="divide-y divide-[var(--hairline)]">
          {plans.map((plan) => <tr key={plan.plan_id} className="hover:bg-surface-2/35"><td className="px-3 py-3"><div className="flex items-center gap-2"><span className="font-medium">{plan.name}</span><Badge className={plan.status === 'active' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'}>{t(`organizationPlans.status.${plan.status}`)}</Badge></div><div className="mt-1 font-mono text-[0.6875rem] text-muted-foreground">{plan.plan_id.slice(0, 8)} · v{plan.version}</div></td><td className="px-3 py-3 text-muted-foreground">{t('organizationPlans.values.capacity', { seats: numberFormat.format(plan.seat_limit), teams: numberFormat.format(plan.team_limit), keys: numberFormat.format(plan.organization_key_limit) })}</td><td className="px-3 py-3 font-mono tabular-nums">{plan.price_currency} {numberFormat.format(plan.price_amount_minor)}<div className="mt-1 font-sans text-[0.6875rem] text-muted-foreground">{plan.price_provider}</div></td><td className="px-3 py-3 text-muted-foreground">{t('organizationPlans.values.days', { count: numberFormat.format(plan.validity_days) })}<div className="mt-1">{t('organizationPlans.values.audit', { count: numberFormat.format(plan.audit_retention_days) })}</div></td><td className="px-3 py-3 text-right">{plan.status === 'active' ? <Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationPlans.actions.disable')} onClick={() => onSelect(plan)}><Ban aria-hidden="true" /></Button> : null}{selected?.plan_id === plan.plan_id ? <span className="sr-only">{t('organizationPlans.values.selected')}</span> : null}</td></tr>)}
        </tbody>
      </table>
    </div>
  )
}

function DisablePlanDialog({ plan, onClose }: { plan?: OrganizationPlan; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useDisableAdminOrganizationPlan()
  const disable = async () => {
    if (!plan) return
    try {
      await mutation.mutateAsync({ planId: plan.plan_id, body: { expected_version: plan.version } })
      onClose()
    } catch {
      // 保留确认区域，管理员可刷新列表后按最新版本重试。
    }
  }
  return (
    <AlertDialog open={plan !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('organizationPlans.disable.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('organizationPlans.disable.body', { name: plan?.name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('organizationPlans.errors.disable')}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('organizationPlans.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className="bg-destructive text-destructive-foreground hover:bg-destructive/90" disabled={mutation.isPending} onClick={(event) => { event.preventDefault(); void disable() }}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Ban aria-hidden="true" />}
            {t('organizationPlans.actions.disable')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function Field({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return <div className="grid gap-2"><Label htmlFor={id}>{label}</Label>{children}</div>
}

function NumberField({ id, label, value, onChange, disabled }: { id: string; label: string; value: string; onChange: (value: string) => void; disabled: boolean }) {
  return <Field id={id} label={label}><Input id={id} type="number" min={1} inputMode="numeric" value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)} /></Field>
}

function LoadingState() {
  return <div className="grid min-h-48 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1/35"><LoaderCircle className="size-5 animate-spin text-muted-foreground" aria-hidden="true" /></div>
}

function StatePanel({ title, body, action }: { title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-48 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1/35 p-6 text-center"><div className="max-w-sm"><Building2 className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}

function toRequest(draft: Draft): OrganizationPlanCreateRequest | undefined {
  const positive = (value: string) => /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0 ? Number(value) : undefined
  const name = draft.name.trim() === draft.name && draft.name.length > 0 && new TextEncoder().encode(draft.name).length <= 80 ? draft.name : undefined
  const description = draft.description === '' ? null : draft.description.trim() === draft.description && new TextEncoder().encode(draft.description).length <= 240 ? draft.description : undefined
  const values = [draft.seatLimit, draft.teamLimit, draft.keyLimit, draft.auditRetentionDays, draft.validityDays, draft.amountMinor].map(positive)
  if (!name || description === undefined || values.some((value) => value === undefined) || values[3]! > 3650 || values[4]! > 3650 || !/^[A-Z]{3}$/.test(draft.currency) || !/^[A-Za-z0-9_.-]{1,32}$/.test(draft.provider)) return undefined
  return {
    name,
    description,
    team_management_enabled: draft.teamManagement,
    sso_enabled: draft.sso,
    custom_roles_enabled: false,
    seat_limit: values[0]!,
    team_limit: values[1]!,
    organization_key_limit: values[2]!,
    audit_retention_days: values[3]!,
    validity_days: values[4]!,
    price_provider: draft.provider,
    price_currency: draft.currency,
    price_amount_minor: values[5]!,
  }
}
