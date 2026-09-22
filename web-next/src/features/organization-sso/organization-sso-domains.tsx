import { Button, Chip, Input, Select, SelectItem, Skeleton } from '@heroui/react'
import { Check, CircleAlert, Clock3, Copy, Globe2, LoaderCircle, RefreshCw, ShieldCheck, ShieldX } from 'lucide-react'
import { useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { copyText } from '@/lib/clipboard'
import type { OrganizationSsoDomain, OrganizationSsoProvider } from '@/lib/api/generated/types.gen'
import {
  useCreateOrganizationSsoDomain,
  useOrganizationSsoDomains,
  useVerifyOrganizationSsoDomain,
} from './organization-sso-api'

const TTL_OPTIONS = [900, 3600, 86_400, 604_800] as const

type IssuedProof = {
  domainId: number
  domain: string
  expectedVersion: number
  txtRecordName: string
  txtRecordValue: string
}

/** 展示企业域名归属事实，并把一次性 TXT 证明限制在当前管理会话内。 */
export function OrganizationSsoDomains({
  organizationId,
  providers,
  canManage,
  providersLoaded,
  query,
}: {
  organizationId: number
  providers: readonly OrganizationSsoProvider[]
  canManage: boolean
  providersLoaded: boolean
  query: ReturnType<typeof useOrganizationSsoDomains>
}) {
  const { t } = useTranslation()
  const createMutation = useCreateOrganizationSsoDomain(organizationId)
  const verifyMutation = useVerifyOrganizationSsoDomain(organizationId)
  const [providerKey, setProviderKey] = useState('')
  const [domain, setDomain] = useState('')
  const [ttl, setTtl] = useState('3600')
  const [issuedProof, setIssuedProof] = useState<IssuedProof>()
  const [copyState, setCopyState] = useState<'name' | 'value'>()
  const [verificationResult, setVerificationResult] = useState<'success' | undefined>()
  const activeProviders = providers.filter((provider) => provider.status === 'active')
  const selectedProviderKey = providerKey || activeProviders[0]?.provider_key || ''

  const createDomain = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!selectedProviderKey || !domain.trim()) return
    setVerificationResult(undefined)
    try {
      const created = await createMutation.mutateAsync({
        domain: domain.trim(),
        provider_key: selectedProviderKey,
        ttl_seconds: Number(ttl),
      })
      setIssuedProof({
        domain: created.domain.domain,
        domainId: created.domain.id,
        expectedVersion: created.domain.version,
        txtRecordName: created.txt_record_name,
        txtRecordValue: created.txt_record_value,
      })
      setDomain('')
    } catch {
      // 保留表单内容，便于用户修正输入或重试同一事实。
    }
  }

  const verifyDomain = async () => {
    if (!issuedProof) return
    try {
      await verifyMutation.mutateAsync({
        body: {
          expected_version: issuedProof.expectedVersion,
          proof_token: issuedProof.txtRecordValue,
        },
        domainId: issuedProof.domainId,
      })
      setIssuedProof(undefined)
      setVerificationResult('success')
    } catch {
      // DNS 尚未传播或证明已过期时保留 TXT 材料，允许在有效期内再次验证。
    }
  }

  const copy = async (target: 'name' | 'value', value: string) => {
    if (await copyText(value)) setCopyState(target)
  }

  return (
    <section className="border-t border-[var(--hairline)]" aria-labelledby="organization-sso-domains-title">
      <header className="flex flex-col gap-3 px-4 py-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-info/10 text-info"><Globe2 className="size-4" aria-hidden="true" /></span>
          <div className="min-w-0">
            <h2 id="organization-sso-domains-title" className="text-base font-semibold">{t('organizationConsole.settings.sso.domains.title')}</h2>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.domains.subtitle')}</p>
          </div>
        </div>
        <Button isIconOnly type="button" size="sm" title={t('organizationConsole.actions.refresh')} variant="light" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={cn('size-3.5', query.isFetching && 'animate-spin')} aria-hidden="true" />
          <span className="sr-only">{t('organizationConsole.actions.refresh')}</span>
        </Button>
      </header>

      {query.isPending ? <DomainLoading /> : null}
      {query.isError ? (
        <DomainState icon={CircleAlert} title={t('organizationConsole.settings.sso.domains.errors.title')} body={t('organizationConsole.settings.sso.domains.errors.body')} action={<Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} />
      ) : null}
      {query.isSuccess && query.data.length === 0 ? <DomainState icon={ShieldX} title={t('organizationConsole.settings.sso.domains.empty.title')} body={t(canManage ? 'organizationConsole.settings.sso.domains.empty.manageBody' : 'organizationConsole.settings.sso.domains.empty.readBody')} /> : null}
      {query.isSuccess && query.data.length > 0 ? <div className="divide-y divide-[var(--hairline)]">{query.data.map((item) => <DomainRow key={item.id} domain={item} />)}</div> : null}

      {canManage && providersLoaded && query.isSuccess ? (
        <div className="border-t border-[var(--hairline)] bg-surface-1/25 px-4 py-4">
          <div className="mb-3 flex items-start gap-2">
            <ShieldCheck className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <div>
              <h3 className="text-sm font-semibold">{t('organizationConsole.settings.sso.domains.createTitle')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.domains.createHint')}</p>
            </div>
          </div>
          {activeProviders.length > 0 ? (
            <form className="grid gap-3 sm:grid-cols-[minmax(0,0.9fr)_minmax(0,1.2fr)_minmax(8rem,0.7fr)_auto] sm:items-end" onSubmit={(event) => void createDomain(event)}>
              <div className="grid gap-1.5">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="organization-sso-domain-provider">{t('organizationConsole.settings.sso.domains.provider')}</label>
                <Select
                  aria-label={t('organizationConsole.settings.sso.domains.provider')}
                  id="organization-sso-domain-provider"
                  isDisabled={createMutation.isPending}
                  items={activeProviders.map((provider) => ({ key: provider.provider_key, label: provider.display_name }))}
                  selectedKeys={[selectedProviderKey]}
                  size="sm"
                  onSelectionChange={(keys) => setProviderKey(String(Array.from(keys)[0] ?? ''))}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              </div>
              <div className="grid gap-1.5">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="organization-sso-domain-name">{t('organizationConsole.settings.sso.domains.domain')}</label>
                <Input autoComplete="off" id="organization-sso-domain-name" isDisabled={createMutation.isPending} isRequired placeholder={t('organizationConsole.settings.sso.domains.domainPlaceholder')} size="sm" value={domain} onChange={(event) => setDomain(event.target.value)} />
              </div>
              <div className="grid gap-1.5">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="organization-sso-domain-ttl">{t('organizationConsole.settings.sso.domains.ttl')}</label>
                <Select
                  aria-label={t('organizationConsole.settings.sso.domains.ttl')}
                  id="organization-sso-domain-ttl"
                  isDisabled={createMutation.isPending}
                  items={TTL_OPTIONS.map((seconds) => ({ key: String(seconds), label: t('organizationConsole.settings.sso.domains.ttlOption', { count: seconds / 60 }) }))}
                  selectedKeys={[ttl]}
                  size="sm"
                  onSelectionChange={(keys) => setTtl(String(Array.from(keys)[0] ?? '3600'))}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              </div>
              <Button type="submit" color="primary" isDisabled={createMutation.isPending || !selectedProviderKey || !domain.trim()}><ShieldCheck className="size-4" aria-hidden="true" />{t('organizationConsole.settings.sso.domains.create')}</Button>
            </form>
          ) : <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.domains.noActiveProvider')}</p>}
          {createMutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('organizationConsole.settings.sso.domains.createError')}</p> : null}
        </div>
      ) : null}

      {issuedProof ? <IssuedProofPanel proof={issuedProof} copyState={copyState} verifyPending={verifyMutation.isPending} verifyError={verifyMutation.isError} onCopy={copy} onVerify={() => void verifyDomain()} /> : null}
      {verificationResult === 'success' ? <p role="status" className="border-t border-[var(--hairline)] px-4 py-3 text-xs text-success">{t('organizationConsole.settings.sso.domains.verifySuccess')}</p> : null}
    </section>
  )
}

function DomainRow({ domain }: { domain: OrganizationSsoDomain }) {
  const { t } = useTranslation()
  const status = domain.status
  const statusClass = status === 'verified' ? 'text-success' : status === 'revoked' ? 'text-destructive' : 'text-warning'
  return (
    <article className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-medium" title={domain.domain}>{domain.domain}</h3>
          <Chip className={cn('shrink-0', statusClass)} size="sm" variant="flat">{t(`organizationConsole.settings.sso.domains.status.${status}`, { defaultValue: status })}</Chip>
        </div>
        <p className="mt-1 truncate font-mono text-xs text-muted-foreground" title={domain.provider_key}>{domain.provider_key}</p>
      </div>
      <dl className="grid grid-cols-2 gap-x-5 gap-y-1 text-xs sm:min-w-64">
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.domains.fields.version')}</dt><dd className="font-mono font-medium">v{domain.version}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.domains.fields.expires')}</dt><dd className="font-medium">{formatTimestamp(domain.proof_expires_at) ?? t('organizationConsole.settings.sso.domains.notAvailable')}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.domains.fields.verified')}</dt><dd className="font-medium">{formatTimestamp(domain.verified_at) ?? t('organizationConsole.settings.sso.domains.notAvailable')}</dd></div>
        <div><dt className="text-muted-foreground">{t('organizationConsole.settings.sso.domains.fields.recheck')}</dt><dd className="font-medium">{formatTimestamp(domain.recheck_at) ?? t('organizationConsole.settings.sso.domains.notAvailable')}</dd></div>
      </dl>
    </article>
  )
}

function IssuedProofPanel({
  proof,
  copyState,
  verifyPending,
  verifyError,
  onCopy,
  onVerify,
}: {
  proof: IssuedProof
  copyState?: 'name' | 'value'
  verifyPending: boolean
  verifyError: boolean
  onCopy: (target: 'name' | 'value', value: string) => void
  onVerify: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="border-t border-[var(--hairline)] bg-info/5 px-4 py-4">
      <div className="flex items-start gap-2">
        <Clock3 className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
        <div className="min-w-0">
          <h3 className="text-sm font-semibold">{t('organizationConsole.settings.sso.domains.proofTitle', { domain: proof.domain })}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.domains.proofHint')}</p>
        </div>
      </div>
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        <ProofField label={t('organizationConsole.settings.sso.domains.txtName')} value={proof.txtRecordName} copied={copyState === 'name'} onCopy={() => onCopy('name', proof.txtRecordName)} />
        <ProofField label={t('organizationConsole.settings.sso.domains.txtValue')} value={proof.txtRecordValue} copied={copyState === 'value'} onCopy={() => onCopy('value', proof.txtRecordValue)} />
      </div>
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <Button type="button" color="primary" size="sm" isDisabled={verifyPending} onClick={onVerify}>
          {verifyPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <ShieldCheck className="size-4" aria-hidden="true" />}
          {t('organizationConsole.settings.sso.domains.verify')}
        </Button>
        {verifyError ? <p role="alert" className="text-xs text-destructive">{t('organizationConsole.settings.sso.domains.verifyError')}</p> : null}
      </div>
    </div>
  )
}

function ProofField({ label, value, copied, onCopy }: { label: string; value: string; copied: boolean; onCopy: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-1.5">
      <span className="text-xs font-medium leading-none text-foreground">{label}</span>
      <div className="flex min-w-0 gap-1.5">
        <Input className="min-w-0" classNames={{ input: 'font-mono text-xs' }} isReadOnly size="sm" value={value} />
        <Button isIconOnly type="button" size="sm" variant="bordered" title={t(copied ? 'organizationConsole.settings.sso.domains.copied' : 'organizationConsole.actions.copy')} aria-label={t(copied ? 'organizationConsole.settings.sso.domains.copied' : 'organizationConsole.actions.copy')} onClick={onCopy}>
          {copied ? <Check className="size-3.5 text-success" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
        </Button>
      </div>
    </div>
  )
}

function DomainLoading() {
  return <div className="grid gap-3 px-4 pb-4" role="status"><Skeleton className="h-14" /><Skeleton className="h-14" /></div>
}

function DomainState({ icon: Icon, title, body, action }: { icon: typeof ShieldX; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-32 place-items-center px-4 pb-5 text-center"><div className="max-w-md"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}

function formatTimestamp(value?: number | null) {
  return value ? new Date(value * 1000).toLocaleString() : undefined
}
