import { useState, type ReactNode } from 'react'
import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { useAccountEligibility, verificationGet, materialBlob, type VerificationMaterial } from './account-verification-api'
import { VerificationAttachment } from './verification-attachment'

export function VerificationMaterials({ endpoint, materials }: { endpoint: string; materials: VerificationMaterial[] }) {
  const { t } = useTranslation()
  return <div className="grid gap-3 md:grid-cols-2">{materials.map((m) => m.content_available === false
    ? <p key={m.id} className="text-sm text-muted-foreground">{m.file_name} · {t('verificationCenter.legacyMissing')}</p>
    : <VerificationAttachment key={`${endpoint}-${m.id}`} name={m.file_name} size={m.size_bytes} contentType={m.content_type} load={(signal) => materialBlob(`${endpoint}/${m.id}`, signal)} />)}</div>
}
function StoredMaterials({ endpoint }: { endpoint: string }) {
  const { t } = useTranslation()
  const query = useQuery({ queryKey: ['verification-materials', endpoint], gcTime: 0, retry: false,
    queryFn: ({ signal }) => verificationGet<VerificationMaterial[]>(endpoint, signal) })
  if (query.isPending) return <p role="status">{t('verificationCenter.loading')}</p>
  if (query.isError) return <p role="alert">{t('verificationCenter.materialError')}</p>
  return <VerificationMaterials endpoint={endpoint} materials={query.data} />
}

export function ProvisioningEligibility({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  const query = useAccountEligibility()
  return <div className="grid gap-5">
    {query.isPending ? <p role="status">{t('verificationCenter.loading')}</p> : query.isError ? <div role="alert"><p>{t('verificationCenter.loadError')}</p><Button type="button" variant="secondary" onClick={() => void query.refetch()}>{t('verificationCenter.refresh')}</Button></div> : query.data.can_apply_for_organization ? children : <section className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-5"><h2 className="text-lg font-semibold">{t('verificationCenter.verifyFirst')}</h2><p className="my-3 text-sm text-muted-foreground">{t('verificationCenter.verifyFirstHint')}</p><a href="#/console/account-verification" className="text-sm text-brand">{t('verificationCenter.openCenter')}</a></section>}
    <ProvisioningHistory />
  </div>
}

function ProvisioningHistory() {
  const { t } = useTranslation()
  const [selected, setSelected] = useState<string>()
  type Entry = { id: number; request_key: string; organization_name: string; status: string; business_reason: string; decision_reason?: string | null; created_at: number; updated_at: number; organization_id?: number | null }
  const query = useInfiniteQuery({ queryKey: ['organization-provisioning', 'self-history'], gcTime: 0, retry: false,
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam, signal }) => verificationGet<{ requests: Entry[]; next_before?: number | null }>(`/api/account/organization-provisioning?limit=25${pageParam ? `&before=${pageParam}` : ''}`, signal),
    getNextPageParam: (p) => p.next_before ?? undefined })
  const rows = query.data?.pages.flatMap((p) => p.requests) ?? []
  return <section className="grid gap-3"><div className="flex items-center justify-between gap-3"><h3 className="font-semibold">{t('verificationCenter.workspaceHistory')}</h3><Button type="button" variant="secondary" size="sm" onClick={() => void query.refetch()}>{t('verificationCenter.refresh')}</Button></div>
    {query.isError ? <p role="alert">{t('verificationCenter.loadError')}</p> : query.isPending ? <p role="status">{t('verificationCenter.loading')}</p> : rows.length === 0 ? <p className="text-sm text-muted-foreground">{t('verificationCenter.empty')}</p> : rows.map((r) => <article key={r.id} className="rounded-xl border border-[var(--hairline)] p-4"><div className="flex flex-wrap items-center justify-between gap-2"><p className="text-sm font-medium">{r.organization_name} · {t(`organizationProvisioning.status.${r.status}`)}</p><Button type="button" size="sm" variant="secondary" onClick={() => setSelected(selected === r.request_key ? undefined : r.request_key)}>{t('verificationCenter.details')}</Button></div><p className="mt-2 text-xs text-muted-foreground">{new Date(r.created_at * 1000).toLocaleString()}</p>{r.status === 'approved' && r.organization_id ? <a className="mt-2 inline-block text-sm text-brand" href={`#/console/organization-onboarding?organization_id=${r.organization_id}`}>{t('verificationCenter.enterWorkspace')}</a> : null}{selected === r.request_key ? <div className="mt-4 grid gap-3 border-t border-[var(--hairline)] pt-4"><p className="whitespace-pre-wrap text-sm">{r.business_reason}</p>{r.decision_reason ? <p className="text-sm">{t('verificationCenter.reviewReason')}: {r.decision_reason}</p> : null}<StoredMaterials endpoint={`/api/account/organization-provisioning/${r.request_key}/materials`} /></div> : null}</article>)}
    {query.hasNextPage ? <Button type="button" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t('verificationCenter.more')}</Button> : null}
  </section>
}

export function OrganizationVerificationHistory({ endpoint }: { endpoint: string }) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  type Entry = { version: number; status: number; summary: string; review_reason?: string | null; updated_at: number; material_ids: number[] }
  const query = useInfiniteQuery({ queryKey: ['organization-verification-history', endpoint], enabled: open, gcTime: 0, retry: false,
    initialPageParam: undefined as number | undefined,
    queryFn: ({ pageParam, signal }) => verificationGet<{ entries: Entry[]; next_cursor?: number | null }>(`${endpoint}${pageParam ? `?before=${pageParam}` : ''}`, signal),
    getNextPageParam: (p) => p.next_cursor ?? undefined })
  const rows = query.data?.pages.flatMap((p) => p.entries) ?? []
  return <details className="rounded-xl border border-[var(--hairline)] p-4" onToggle={(e) => setOpen(e.currentTarget.open)}><summary className="cursor-pointer text-sm font-medium">{t('verificationCenter.history')}</summary><div className="mt-3 grid gap-3"><p className="text-xs text-muted-foreground">{t('verificationCenter.legacyHistoryHint')}</p>{query.isPending ? <p role="status">{t('verificationCenter.loading')}</p> : query.isError ? <p role="alert">{t('verificationCenter.loadError')}</p> : rows.map((r) => <article key={r.version} className="border-t border-[var(--hairline)] pt-3 text-sm"><p>v{r.version} · {t(`verificationCenter.status.${r.status}`)} · {new Date(r.updated_at * 1000).toLocaleString()}</p><p className="mt-2 whitespace-pre-wrap">{r.summary}</p>{r.review_reason ? <p>{t('verificationCenter.reviewReason')}: {r.review_reason}</p> : null}<p className="mt-1 text-xs text-muted-foreground">{t('verificationCenter.materials')}: {r.material_ids.map((id) => `#${id}`).join(', ')}</p></article>)}{query.hasNextPage ? <Button type="button" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t('verificationCenter.more')}</Button> : null}</div></details>
}
