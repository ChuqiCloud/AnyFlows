import { useState, type FormEvent } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { LoaderCircle, Settings2, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { apiClient, ApiError } from '@/lib/api'
import { sessionSecurity } from '@/features/account-verification/account-verification-api'

type Entitlement = { version: number; team_management_enabled: boolean; sso_enabled: boolean; custom_roles_enabled: boolean; seat_limit: number; team_limit: number; organization_key_limit: number; audit_retention_days: number; valid_from: number; valid_until: number; grace_until: number | null }
const capabilities = ['team_management_enabled', 'sso_enabled', 'custom_roles_enabled'] as const
const capacities = ['seat_limit', 'team_limit', 'organization_key_limit', 'audit_retention_days'] as const
const dates = ['valid_from', 'valid_until', 'grace_until'] as const

/** Platform administrators edit entitlements without impersonating an organization member. */
export function PlatformEntitlementEditor({ organizationId, name, onClose }: { organizationId: number; name: string; onClose: () => void }) {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ['platform-entitlement', organizationId], gcTime: 0, retry: false,
    queryFn: async ({ signal }) => {
      const result = await apiClient.get<Entitlement>({ url: `/api/platform/organizations/${organizationId}/entitlement`, headers: { 'x-anyflows-platform-reason': 'Review organization entitlement configuration' }, signal, security: [...sessionSecurity] }) as unknown as { data: Entitlement }
      return result.data
    } })
  return <section className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-5" aria-label={t('enterpriseEntitlement.title')}><header className="mb-4 flex items-center justify-between gap-3"><h3 className="flex items-center gap-2 font-semibold"><Settings2 className="size-4" />{t('enterpriseEntitlement.title')} · {name}</h3><Button type="button" size="sm" variant="ghost" onClick={onClose}><X className="size-4" />{t('verificationCenter.close')}</Button></header>{query.isPending ? <p role="status">{t('verificationCenter.loading')}</p> : query.isError ? <div role="alert"><p>{t('verificationCenter.loadError')}</p><Button type="button" onClick={() => void query.refetch()}>{t('verificationCenter.refresh')}</Button></div> : <EntitlementForm key={query.data.version} initial={query.data} organizationId={organizationId} onRefresh={() => void query.refetch()} />}</section>
}
function EntitlementForm({ initial, organizationId, onRefresh }: { initial: Entitlement; organizationId: number; onRefresh: () => void }) {
  const { t } = useTranslation()
  const [value, setValue] = useState(initial)
  const [reason, setReason] = useState('')
  const [saved, setSaved] = useState(false)
  const client = useQueryClient()
  const mutation = useMutation({ mutationFn: async () => {
    await apiClient.patch({ url: `/api/platform/organizations/${organizationId}/entitlement`, security: [...sessionSecurity], body: {
      expected_version: initial.version, reason: reason.trim(),
      team_management_enabled: value.team_management_enabled, sso_enabled: value.sso_enabled, custom_roles_enabled: value.custom_roles_enabled,
      seat_limit: value.seat_limit, team_limit: value.team_limit, organization_key_limit: value.organization_key_limit,
      audit_retention_days: value.audit_retention_days, valid_from: value.valid_from, valid_until: value.valid_until, grace_until: value.grace_until,
    } })
  }, onSuccess: async () => {
    setSaved(true)
    await Promise.all([client.invalidateQueries({ queryKey: ['platform-organizations'] }), client.invalidateQueries({ queryKey: ['organization-directory'] }), client.invalidateQueries({ queryKey: ['platform-organization-entitlement'] }), client.invalidateQueries({ queryKey: ['organization'] })])
  } })
  const valid = capacities.every((key) => Number.isSafeInteger(value[key]) && value[key] >= 1)
    && value.audit_retention_days <= 3650 && value.valid_until > value.valid_from
    && (value.grace_until === null || value.grace_until >= value.valid_until) && reason.trim().length > 0
  const submit = async (e: FormEvent) => { e.preventDefault(); if (!valid) return; try { await mutation.mutateAsync() } catch { /* Preserve entered values after CAS conflicts. */ } }
  return <form onSubmit={(e) => void submit(e)} className="grid gap-4"><p className="text-sm text-muted-foreground">{t('enterpriseEntitlement.description')}</p>
    <div className="grid gap-3 sm:grid-cols-3">{capabilities.map((key) => <label key={key} className="flex items-center gap-2 rounded-lg border border-[var(--hairline)] p-3 text-sm"><input type="checkbox" checked={value[key]} disabled={saved} onChange={(e) => setValue({ ...value, [key]: e.target.checked })} />{t(`enterpriseEntitlement.${key}`)}</label>)}</div>
    {initial.sso_enabled && !value.sso_enabled ? <p className="rounded-lg bg-warning/10 p-3 text-sm text-warning">{t('enterpriseEntitlement.disableSsoHint')}</p> : null}
    <div className="grid gap-3 sm:grid-cols-2">{capacities.map((key) => <label key={key} className="grid gap-2 text-sm"><span>{t(`enterpriseEntitlement.${key}`)}</span><Input aria-label={t(`enterpriseEntitlement.${key}`)} type="number" min={1} max={key === 'audit_retention_days' ? 3650 : undefined} required disabled={saved} value={String(value[key])} onChange={(e) => setValue({ ...value, [key]: Number(e.target.value) })} /></label>)}{dates.map((key) => <label key={key} className="grid gap-2 text-sm"><span>{t(`enterpriseEntitlement.${key}`)}</span><Input aria-label={t(`enterpriseEntitlement.${key}`)} type="datetime-local" required={key !== 'grace_until'} disabled={saved} value={toLocalDate(value[key])} onChange={(e) => setValue({ ...value, [key]: e.target.value ? Math.floor(new Date(e.target.value).getTime() / 1000) : key === 'grace_until' ? null : 0 })} /></label>)}</div>
    <Textarea aria-label={t('enterpriseEntitlement.reason')} required maxLength={512} disabled={saved} placeholder={t('enterpriseEntitlement.reason')} value={reason} onChange={(e) => setReason(e.target.value)} />
    {mutation.isError ? <div role="alert" className="text-sm text-destructive"><p>{t(mutation.error instanceof ApiError && mutation.error.status === 409 ? 'verificationCenter.conflict' : 'enterpriseEntitlement.error')}</p><Button type="button" variant="secondary" onClick={onRefresh}>{t('verificationCenter.refresh')}</Button></div> : null}
    {saved ? <div role="status" className="flex items-center gap-3 text-sm text-success">{t('enterpriseEntitlement.saved')}<Button type="button" variant="secondary" onClick={onRefresh}>{t('enterpriseEntitlement.continueEditing')}</Button></div> : <Button type="submit" className="justify-self-start" disabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : null}{t('enterpriseEntitlement.save')}</Button>}
  </form>
}
function toLocalDate(seconds: number | null) { if (seconds === null) return ''; const date = new Date(seconds * 1000); if (Number.isNaN(date.getTime())) return ''; return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16) }
