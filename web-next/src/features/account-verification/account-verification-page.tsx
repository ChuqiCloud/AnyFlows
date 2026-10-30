import { useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { CheckCircle2, ChevronLeft, ChevronRight, Clock3, FileCheck2, LoaderCircle, RefreshCw, ShieldAlert, XCircle } from 'lucide-react'
import { Button, Chip, Select, SelectItem, Skeleton } from '@heroui/react'
import { Input } from '@heroui/react'
import { Textarea } from '@heroui/react'
import { ApiError } from '@/lib/api'
import { useConsoleExtensionEntryPoint } from '@/extensions/query'
import { cn } from '@/lib/utils'
import { useAccountEligibility, useVerifiedAccount, useAccountVerifications, useVerificationDetail, useSubmitAccountVerification,
  useReviewAccountVerification, materialBlob, verificationBase, type VerificationKind, type VerificationProvider, type VerificationRecord } from './account-verification-api'
import { VerifiedAccount } from './verified-account'
import { VerificationInfoCard, VerificationProgress } from './verification-progress'
import { VerificationKindPicker } from './verification-kind-picker'
import { maskedVerificationName } from './verification-identity'
import { AlipayVerification } from './alipay-verification'
import { VerificationAttachment } from './verification-attachment'

const countryOptions = ['CN', 'HK', 'MO', 'TW', 'US', 'CA', 'GB', 'JP', 'KR', 'SG', 'AU', 'DE', 'FR'] as const
const manualDocumentTypes = ['national_id', 'passport', 'residence_permit', 'foreign_passport', 'hkm_macao_pass', 'taiwan_pass', 'other'] as const

function documentTypes(kind: VerificationKind, provider: VerificationProvider) {
  if (kind === 'enterprise') return ['business_registration'] as const
  // Alipay's current certdoc.certverify.preconsult contract also supports IDENTITY_CARD only.
  return provider === 'alipay' ? ['national_id'] as const : manualDocumentTypes
}

/*
 * 页面结构：顶部状态卡（一目了然告诉用户当前在哪一步）+ 历史记录列表。
 * 用户态：未认证/被驳回/待补充 → 状态卡内嵌分步表单；已认证 → 显示 VerifiedAccount。
 * 管理员态：跳过状态卡，直接进审核队列。
 */
export function AccountVerificationPage({ admin = false }: { admin?: boolean }) {
  const { t } = useTranslation()
  const [status, setStatus] = useState<number | undefined>(admin ? 1 : undefined)
  const [selected, setSelected] = useState<number>()
  const [formRestart, setFormRestart] = useState(0)
  const [replacing, setReplacing] = useState(false)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(10)
  // 未认证时用户选择的认证主体；undefined 表示还停在"选择个人/企业"首屏。
  const [choice, setChoice] = useState<VerificationKind>()
  const pendingScroll = useRef<number | undefined>(undefined)
  const verified = useVerifiedAccount(!admin)
  const query = useAccountVerifications(admin, status)
  const entries = query.data?.pages.flatMap((p) => p.cases) ?? []
  // 远端还有下一页时把已知总页数 +1，让分页条能翻到未下载的那一页。
  const pageCount = Math.max(1, Math.ceil((entries.length + (query.hasNextPage ? 1 : 0)) / pageSize))
  const currentPage = Math.min(page, pageCount)
  const visibleEntries = entries.slice((currentPage - 1) * pageSize, currentPage * pageSize)
  // 还有远端页时，翻到最后一页继续往后会自动拉取下一页。
  const goToPage = (next: number) => {
    const loadedPages = Math.ceil(entries.length / pageSize)
    if (next > loadedPages && query.hasNextPage && !query.isFetchingNextPage) {
      void query.fetchNextPage()
    }
    setPage(next)
  }

  useEffect(() => {
    if (selected === undefined || pendingScroll.current !== selected) return
    const element = document.getElementById(`verification-case-${selected}`)
    if (element) { element.scrollIntoView({ block: 'nearest' }); pendingScroll.current = undefined }
  }, [selected, query.data])

  const latest = entries[0]
  // 已认证记录由 verified 接口维护，历史列表里只看 process 中的记录。
  const pending = !admin && latest && (latest.status === 1 || latest.status === 2 || latest.status === 3) ? latest : undefined
  const showForm = !admin && !verified.data && !pending

  return <div className="grid min-w-0 gap-5">
    <header className="flex items-start justify-between gap-3">
      <div>
        <h2 className="flex items-center gap-2 text-xl font-semibold"><FileCheck2 className="size-5 text-primary" />{t(admin ? 'verificationCenter.adminTitle' : 'verificationCenter.title')}</h2>
        <p className="mt-2 text-sm text-default-500">{t(admin ? 'verificationCenter.adminDescription' : 'verificationCenter.description')}</p>
      </div>
      <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => { void query.refetch(); if (!admin) void verified.refetch() }}><RefreshCw className="size-4" />{t('verificationCenter.refresh')}</Button>
    </header>
    {admin ? <nav aria-label={t('verificationCenter.adminLinks')} className="flex flex-wrap gap-3 text-sm"><a className="text-primary underline-offset-4 hover:underline" href="/console/system-settings/account-verification">{t('verificationCenter.settingsTitle')}</a><a className="text-primary underline-offset-4 hover:underline" href="/console/account-verification">{t('verificationCenter.myVerification')}</a></nav> : null}

    {!admin ? (
      verified.isPending ? <StatusCardSkeleton /> : verified.isError ? (
        <p role="alert" className="rounded-xl border border-danger/25 bg-danger/8 p-4 text-sm text-danger">{t('verificationCenter.loadError')}</p>
      ) : verified.data && !replacing ? (
        <VerifiedAccount record={verified.data} onReplace={() => { setReplacing(true); setFormRestart((value) => value + 1) }} />
      ) : pending ? (
        <ResultCard
          status={pending.status}
          providerStatus={pending.provider_status}
          record={pending}
          actions={<>
            {pending.status === 3 ? (
              <Button color="primary" size="sm" onClick={() => { setReplacing(true); setFormRestart((v) => v + 1) }}>
                {t('verificationCenter.supplementAction')}
              </Button>
            ) : null}
            <Button size="sm" variant="light" onClick={() => setSelected(pending.id)}>{t('verificationCenter.details')}</Button>
          </>}
        />
      ) : showForm || replacing ? (
        <div className="grid gap-3">
          {verified.data || pending ? (
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-default-200 bg-content1 p-4">
              <p className="text-sm text-default-500">{t('verificationCenter.replacementHint')}</p>
              <Button type="button" size="sm" variant="bordered" onClick={() => setReplacing(false)}>{t('verificationCenter.backToResult')}</Button>
            </div>
          ) : null}
          <StatusCard
            description={t('verificationCenter.notVerifiedHint')}
            icon={<ShieldAlert className="size-5 text-warning" aria-hidden="true" />}
            title={t('verificationCenter.notVerifiedTitle')}
            tone="warning"
          >
            {choice === undefined ? null : <Button size="sm" variant="light" onClick={() => setChoice(undefined)}>{t('verificationCenter.back')}</Button>}
          </StatusCard>
          {/* 未认证首屏先选个人/企业，选定后再展开分步表单；"更换认证"直接进表单不重复选择。 */}
          {choice === undefined && !replacing ? (
            <VerificationKindPicker kind="individual" onPick={setChoice} />
          ) : (
            <AccountVerificationForm
              initialKind={choice ?? 'individual'}
              key={formRestart}
              restartSignal={formRestart}
              onSubmitted={(id) => { setStatus(undefined); setSelected(id); pendingScroll.current = id; setReplacing(false) }}
            />
          )}
        </div>
      ) : null
    ) : null}

    {/* 历史记录：用户态下 pending 时跟随状态卡的骨架，不再单独渲染一层 */}
    {admin || !verified.isPending ? (
      <section className="grid gap-3" aria-labelledby="verification-history">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <h3 id="verification-history" className="text-sm font-semibold uppercase tracking-[0.08em] text-default-500">{t('verificationCenter.history')}</h3>
          <Select
            aria-label={t('verificationCenter.filter')}
            className="w-40"
            items={[
              { key: 'all', label: t('verificationCenter.all') },
              ...[1, 3, 4, 5].map((s) => ({ key: String(s), label: t(`verificationCenter.status.${s}`) })),
            ]}
            selectedKeys={[status === undefined ? 'all' : String(status)]}
            size="sm"
            onSelectionChange={(keys) => {
              const value = String(Array.from(keys)[0] ?? 'all')
              setStatus(value === 'all' ? undefined : Number(value))
              setPage(1)
              setSelected(undefined)
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </div>
        {query.isPending ? <HistorySkeleton /> : query.isError ? (
          <p role="alert" className="rounded-xl border border-danger/25 bg-danger/8 p-4 text-sm text-danger">{t('verificationCenter.loadError')}</p>
        ) : visibleEntries.length === 0 ? (
          <p className="rounded-xl border border-dashed border-default-200 p-8 text-center text-sm text-default-500">{t('verificationCenter.empty')}</p>
        ) : (
          <ol className="grid gap-2">
            {visibleEntries.map((item) => {
              const expanded = selected === item.id
              return (
                <li key={item.id} id={`verification-case-${item.id}`}>
                  <article className={cn('overflow-hidden rounded-xl border transition-colors', expanded ? 'border-primary/40 bg-primary/5' : 'border-default-200 bg-content1 hover:border-default-300')}>
                    <button
                      type="button"
                      aria-expanded={expanded}
                      className="grid w-full grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-4 py-3 text-left"
                      onClick={() => setSelected(expanded ? undefined : item.id)}
                    >
                      <StatusDot status={item.status} providerStatus={item.provider_status} />
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">
                          {admin ? item.subject_name : maskedVerificationName(item.subject_name)}
                          <span className="ml-2 text-xs font-normal text-default-500">
                            {t(`verificationCenter.${item.kind}`)} · #{item.id}{admin ? ` · UID ${item.user_id}` : ''}
                          </span>
                        </p>
                        <p className="mt-0.5 text-xs text-default-500">
                          {new Date(item.created_at * 1000).toLocaleString()}
                          {item.provider_status ? ` · ${t(`verificationCenter.providerStatuses.${item.provider_status}`, { defaultValue: item.provider_status })}` : ''}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        <StatusChip status={item.status} providerStatus={item.provider_status} />
                        <ChevronRight className={cn('size-4 text-default-400 transition-transform', expanded && 'rotate-90')} aria-hidden="true" />
                      </div>
                    </button>
                    {expanded ? (
                      <div className="border-t border-default-200 px-4 py-4">
                        <VerificationDetailPanel key={`${admin}-${item.id}`} id={item.id} admin={admin} onRestart={() => { setStatus(undefined); setSelected(undefined); setReplacing(true); setFormRestart((value) => value + 1) }} />
                      </div>
                    ) : null}
                  </article>
                </li>
              )
            })}
          </ol>
        )}
        {entries.length > 0 ? (
          <VerificationPagination
            fetching={query.isFetchingNextPage}
            page={currentPage}
            pageSize={pageSize}
            total={Math.max(1, entries.length + (query.hasNextPage ? 1 : 0))}
            onPage={(next) => { goToPage(next); setSelected(undefined) }}
            onPageSize={(size) => { setPageSize(size); setPage(1); setSelected(undefined) }}
          />
        ) : null}
      </section>
    ) : null}
  </div>
}

/** 与后台一致的分页条：上一页 / 页码 / 下一页 + 每页条数。 */
function VerificationPagination({ page, pageSize, total, fetching, onPage, onPageSize }: {
  page: number
  pageSize: number
  total: number
  fetching: boolean
  onPage: (page: number) => void
  onPageSize: (size: number) => void
}) {
  const { t } = useTranslation()
  // total 超过已加载条数时至少暴露一页可继续拉取。
  const pages = Math.max(1, Math.ceil(total / pageSize))
  const items = Array.from({ length: Math.min(pages, 7) }, (_, index) => {
    // 页数超过 7 时以当前页为中心开窗，避免页码条无限增长。
    if (pages <= 7) return index + 1
    const start = Math.min(Math.max(page - 3, 1), pages - 6)
    return start + index
  })
  return (
    <nav className="flex flex-wrap items-center justify-between gap-3 border-t border-default-200 pt-3 text-xs" aria-label={t('verificationCenter.pagination')}>
      <div className="flex items-center gap-2">
        <span className="text-default-500">{t('verificationCenter.perPage')}</span>
        <Select
          aria-label={t('verificationCenter.perPage')}
          className="w-20"
          items={[10, 20, 50].map((value) => ({ key: String(value), label: String(value) }))}
          selectedKeys={[String(pageSize)]}
          size="sm"
          onSelectionChange={(keys) => onPageSize(Number(Array.from(keys)[0] ?? pageSize))}
        >
          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
        </Select>
        <span className="text-default-500">{t('verificationCenter.totalItems', { total })}</span>
      </div>
      <div className="flex items-center gap-1">
        <Button isIconOnly aria-label={t('verificationCenter.previous')} isDisabled={page <= 1 || fetching} size="sm" variant="light" onClick={() => onPage(page - 1)}>
          <ChevronLeft className="size-4" aria-hidden="true" />
        </Button>
        {items.map((value) => (
          <Button key={value} aria-current={value === page ? 'page' : undefined} className="min-w-8" size="sm" variant={value === page ? 'solid' : 'light'} color={value === page ? 'primary' : 'default'} onClick={() => onPage(value)}>{value}</Button>
        ))}
        <Button isIconOnly aria-label={t('verificationCenter.nextPage')} isDisabled={page >= pages || fetching} size="sm" variant="light" onClick={() => onPage(page + 1)}>
          <ChevronRight className="size-4" aria-hidden="true" />
        </Button>
      </div>
    </nav>
  )
}

/** 历史/详情查询共享的更轻量骨架：模拟一条记录行。 */
function HistorySkeleton() {
  return (
    <div role="status" aria-busy="true" className="grid gap-2">
      {[0, 1, 2].map((key) => (
        <div key={key} className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-xl border border-default-200 bg-content1 px-4 py-3">
          <Skeleton className="size-2.5 rounded-full" />
          <div className="grid gap-2"><Skeleton className="h-3.5 w-1/2 rounded" /><Skeleton className="h-3 w-1/3 rounded" /></div>
          <Skeleton className="h-5 w-16 rounded-full" />
        </div>
      ))}
    </div>
  )
}

function StatusCardSkeleton() {
  return (
    <div role="status" aria-busy="true" className="grid gap-3 rounded-2xl border border-default-200 bg-content1 p-5">
      <Skeleton className="h-5 w-40 rounded" />
      <Skeleton className="h-4 w-2/3 rounded" />
    </div>
  )
}

type StatusTone = 'info' | 'warning' | 'success' | 'danger'

const STATUS_TONE: Record<StatusTone, { wrap: string; icon: string }> = {
  info: { wrap: 'border-info/25 bg-info/6', icon: 'text-info' },
  warning: { wrap: 'border-warning/25 bg-warning/6', icon: 'text-warning' },
  success: { wrap: 'border-success/25 bg-success/6', icon: 'text-success' },
  danger: { wrap: 'border-danger/25 bg-danger/6', icon: 'text-danger' },
}

function StatusCard({ icon, title, tone, children, description, record }: { icon: ReactNode; title: string; tone: StatusTone; children?: ReactNode; description?: string; record?: { id: number; kind: VerificationKind } }) {
  const { t } = useTranslation()
  return (
    <section aria-label={title} className={cn('rounded-2xl border p-5', STATUS_TONE[tone].wrap)}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className={cn('grid size-9 shrink-0 place-items-center rounded-xl border border-current/20 bg-background/40', STATUS_TONE[tone].icon)}>{icon}</span>
          <div className="min-w-0">
            <h3 className="text-base font-semibold">{title}</h3>
            {record ? <p className="mt-0.5 text-xs text-default-500">{t(`verificationCenter.${record.kind}`)} · #{record.id}</p> : null}
            {description ? <p className="mt-2 max-w-prose text-sm leading-6 text-default-500">{description}</p> : null}
          </div>
        </div>
        {children ? <div className="flex items-center gap-2">{children}</div> : null}
      </div>
    </section>
  )
}

/** 与后台实名页一致的状态卡：页头图标 + 标题 + 副标题，下方三项进度条与信息卡。 */
function ResultCard({ record, status, providerStatus, actions }: {
  record: VerificationRecord
  status: number
  providerStatus?: string | null
  actions?: ReactNode
}) {
  const { t } = useTranslation()
  const approved = status === 4
  const failed = status === 5
  const tone: StatusTone = failed ? 'danger' : approved ? 'success' : 'info'
  const Icon = failed ? XCircle : approved ? CheckCircle2 : Clock3
  const titleKey = failed ? 'resultFailedTitle' : approved ? 'resultApprovedTitle' : `resultPendingTitle.${status}`
  const hintKey = failed ? 'resultFailedHint' : approved ? 'resultApprovedHint' : `pendingHints.${status}`
  return (
    <section aria-label={t(titleKey)} className={cn('rounded-2xl border', STATUS_TONE[tone].wrap)}>
      <header className="flex flex-col items-center gap-2 px-6 pb-5 pt-7 text-center">
        <span className={cn('grid size-14 place-items-center rounded-2xl border border-current/20 bg-background/40', STATUS_TONE[tone].icon)}>
          <Icon className="size-7" aria-hidden="true" />
        </span>
        <h2 className="text-lg font-semibold">{t(titleKey)}</h2>
        <p className="max-w-prose text-sm leading-6 text-default-500">{t(hintKey, { defaultValue: t(`pendingHints.${status}`) })}</p>
      </header>
      <div className="grid gap-4 border-t border-current/10 px-6 py-5">
        <VerificationProgress approved={approved} providerStatus={providerStatus} status={status} />
        <VerificationInfoCard
          documentNumberMasked={record.document_number_masked}
          documentType={record.document_type}
          id={record.id}
          kind={record.kind}
          subjectName={maskedVerificationName(record.subject_name)}
          updatedAt={record.updated_at}
        />
        {actions ? <div className="flex flex-wrap justify-center gap-2">{actions}</div> : null}
      </div>
    </section>
  )
}

function StatusDot({ status, providerStatus }: { status: number; providerStatus?: string | null }) {
  const tone = providerStatus === 'expired'
    ? 'bg-default-400'
    : status === 4
      ? 'bg-success'
      : status === 5
        ? 'bg-danger'
        : status === 3
          ? 'bg-warning'
          : 'bg-info'
  return <span aria-hidden="true" className={cn('size-2.5 rounded-full', tone)} />
}

function StatusChip({ status, providerStatus }: { status: number; providerStatus?: string | null }) {
  const { t } = useTranslation()
  const label = providerStatus === 'expired' ? t('verificationCenter.providerStatuses.expired') : t(`verificationCenter.status.${status}`)
  const cls = providerStatus === 'expired'
    ? 'bg-default-100 text-default-600'
    : status === 4
      ? 'bg-success/10 text-success'
      : status === 5
        ? 'bg-danger/10 text-danger'
        : status === 3
          ? 'bg-warning/10 text-warning'
          : 'bg-info/10 text-info'
  return <Chip className={cls} size="sm" variant="flat">{label}</Chip>
}

function VerificationLoading() {
  return <HistorySkeleton />
}

function AccountVerificationForm({ restartSignal, initialKind = 'individual', onSubmitted }: { restartSignal: number; initialKind?: VerificationKind; onSubmitted: (id: number) => void }) {
  const { t } = useTranslation()
  const eligibility = useAccountEligibility()
  const verifiedAccountEntry = useConsoleExtensionEntryPoint('account-verification')
  const mutation = useSubmitAccountVerification()
  const [step, setStep] = useState<0 | 1 | 2>(0)
  // 认证主体由首屏选择卡决定，表单内不再重复选择。
  const [kind] = useState<VerificationKind>(initialKind)
  const [provider, setProvider] = useState<VerificationProvider>(restartSignal > 0 ? 'alipay' : 'manual')
  const [name, setName] = useState('')
  const [country, setCountry] = useState('CN')
  const [documentType, setDocumentType] = useState('national_id')
  const [documentNumber, setDocumentNumber] = useState('')
  const [summary, setSummary] = useState('')
  const [files, setFiles] = useState<File[]>([])
  const [fileError, setFileError] = useState(false)
  const availableProviders = useMemo(() => (eligibility.data?.providers ?? []).filter((value): value is VerificationProvider => value === 'manual' || value === 'alipay'), [eligibility.data?.providers])
  const providerOptions = useMemo(() => (kind === 'enterprise' ? eligibility.data?.enterprise_providers ?? availableProviders.filter((value) => value === 'manual') : eligibility.data?.individual_providers ?? availableProviders).filter((value): value is VerificationProvider => value === 'manual' || value === 'alipay'), [kind, eligibility.data, availableProviders])
  const reasonRequired = kind === 'enterprise' ? eligibility.data?.enterprise_reason_required ?? true : eligibility.data?.individual_reason_required ?? true
  const availableDocumentTypes = documentTypes(kind, provider)
  useEffect(() => {
    if (eligibility.data && providerOptions.length > 0 && !providerOptions.includes(provider)) setProvider(providerOptions[0])
  }, [eligibility.data, kind, providerOptions, provider])
  useEffect(() => {
    if (restartSignal === 0) return
    setStep(0)
    const form = document.getElementById('verification-form')
    form?.scrollIntoView({ block: 'start' })
    form?.focus({ preventScroll: true })
  }, [restartSignal])
  useEffect(() => {
    setFiles([])
    setFileError(false)
    setDocumentNumber('')
    if (provider === 'alipay') { setCountry('CN'); setDocumentType('national_id') }
  }, [provider, kind])
  useEffect(() => {
    if (!availableDocumentTypes.includes(documentType as never)) setDocumentType(availableDocumentTypes[0])
  }, [availableDocumentTypes, documentType])
  const stepOneValid = name.trim().length > 0 && country.length === 2 && documentNumber.trim().length >= 5
  const stepTwoValid = provider === 'alipay' || (files.length > 0 && files.length <= 5 && files.every((f) => f.size > 0 && f.size <= 10_000_000 && ['application/pdf', 'image/png', 'image/jpeg', 'image/webp'].includes(f.type)) && files.reduce((sum, f) => sum + f.size, 0) <= 25_000_000)
  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (eligibility.isError || !providerOptions.includes(provider) || mutation.isPending) return
    const valid = provider === 'alipay' ? files.length === 0 : stepTwoValid
    setFileError(!valid)
    if (!valid) return
    try { const record = await mutation.mutateAsync({ kind, provider, document_country: country, document_type: kind === 'enterprise' ? 'business_registration' : documentType, document_number: documentNumber.trim() || undefined, subject_name: name.trim(), summary: summary.trim(), files }); setFiles([]); setName(''); setDocumentNumber(''); setSummary(''); setStep(0); onSubmitted(record.id) } catch { /* Keep the form for correction. */ }
  }
  return <section id="verification-form" tabIndex={-1} className="rounded-2xl border border-default-200 bg-content1 p-6 sm:p-8">
    {eligibility.isSuccess && eligibility.data.enterprise_verified ? <div className="mb-5 flex flex-wrap items-center justify-between gap-3 rounded-lg bg-success/10 p-3"><p className="flex items-center gap-2 text-sm text-success"><CheckCircle2 className="size-4" />{t('verificationCenter.enterpriseGranted')}</p>{verifiedAccountEntry ? <a href={'' + verifiedAccountEntry.path} className="text-sm font-medium text-primary">{t('verificationCenter.applyWorkspace')}</a> : null}</div> : null}
    {eligibility.isPending ? <VerificationLoading /> : eligibility.isError ? <p role="alert" className="text-sm text-danger">{t('verificationCenter.loadError')}</p> : null}

    <header className="text-center">
      <h3 className="text-base font-semibold">{t(kind === 'enterprise' ? 'verificationCenter.enterpriseCertification' : 'verificationCenter.personalCertification')}</h3>
      <p className="mt-1 text-xs text-default-500">{t(`verificationCenter.kindHints.${kind}`)}</p>
    </header>

    {/* 流程进度：三步横向排成一条进度条，已完成打勾、当前步高亮。 */}
    <ol className="mx-auto mt-6 grid w-full max-w-lg grid-cols-3" aria-label={t('verificationCenter.stepsLabel')}>
      {([0, 1, 2] as const).map((value) => {
        const done = step > value
        const active = step === value
        return (
          <li key={value} aria-current={active ? 'step' : undefined} className="relative flex flex-col items-center gap-1.5">
            {/* 连接线：从前一个圆点中心连到当前圆点中心，压到圆点下方。 */}
            {value > 0 ? <span aria-hidden="true" className={cn('absolute right-1/2 top-3 h-px w-full -translate-x-3', done ? 'bg-success/40' : 'bg-default-200')} /> : null}
            <span className={cn(
              'relative z-10 grid size-6 place-items-center rounded-full text-[0.625rem] font-semibold',
              done && 'bg-success text-success-foreground',
              active && 'bg-primary text-primary-foreground',
              !done && !active && 'bg-default-200 text-default-500',
            )}>{done ? <CheckCircle2 className="size-3.5" aria-hidden="true" /> : value + 1}</span>
            <span className={cn('whitespace-nowrap text-xs', active ? 'font-semibold text-foreground' : done ? 'text-success' : 'text-default-500')}>
              {t(`verificationCenter.steps.${value}`)}
            </span>
          </li>
        )
      })}
    </ol>

    {/* 表单字段：与后台实名页一致，标签在上、控件占满四列栅格，居中窄栏避免超宽输入。 */}
    <form onSubmit={(event) => void submit(event)} className="mx-auto grid max-w-xl gap-5">
      {step === 0 ? (
        <div className="grid gap-2">
          <span className="text-sm font-medium">{t('verificationCenter.provider')}</span>
          <div className="grid gap-2 sm:grid-cols-2">
            {providerOptions.map((value) => (
              <button key={value} type="button" aria-pressed={provider === value} onClick={() => { setProvider(value); mutation.reset() }} className={cn('rounded-xl border p-4 text-left transition', provider === value ? 'border-primary/50 bg-primary/5' : 'border-default-200 hover:border-default-300')}>
                <span className="block text-sm font-medium">{t(`verificationCenter.providers.${value}`)}</span>
                <span className="mt-1 block text-xs leading-5 text-default-500">{t(`verificationCenter.providerHints.${value}`)}</span>
              </button>
            ))}
            {providerOptions.length === 0 ? <p className="text-xs text-danger">{t('verificationCenter.noProvider')}</p> : null}
          </div>
        </div>
      ) : null}

      {step === 1 ? (
        <>
          <label className="grid gap-2 text-sm"><span>{t(kind === 'enterprise' ? 'verificationCenter.companyName' : 'verificationCenter.personName')}</span><Input aria-label={t('verificationCenter.subjectName')} isRequired maxLength={128} value={name} onChange={(e) => setName(e.target.value)} /></label>
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="grid gap-2 text-sm"><span>{t('verificationCenter.documentCountry')}</span><Input aria-label={t('verificationCenter.documentCountry')} list="verification-countries" value={country} isDisabled={provider === 'alipay'} isRequired minLength={2} maxLength={2} pattern="[A-Z]{2}" placeholder="CN" onChange={(e) => setCountry(e.target.value.toUpperCase())} /><datalist id="verification-countries">{countryOptions.map((value) => <option key={value} value={value}>{t(`verificationCenter.countries.${value}`)}</option>)}</datalist></label>
            <div className="grid gap-2 text-sm"><span>{t('verificationCenter.documentType')}</span>
              <Select
                aria-label={t('verificationCenter.documentType')}
                isDisabled={kind === 'enterprise' || provider === 'alipay'}
                items={availableDocumentTypes.map((value) => ({ key: value, label: t(`verificationCenter.documentTypes.${value}`) }))}
                selectedKeys={[kind === 'enterprise' ? 'business_registration' : documentType]}
                size="sm"
                onSelectionChange={(keys) => setDocumentType(String(Array.from(keys)[0] ?? documentType))}
              >
                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
              </Select>
              {provider === 'alipay' ? <span className="text-xs text-default-500">{t('verificationCenter.alipayDocumentTypeHint')}</span> : null}
            </div>
          </div>
          <label className="grid gap-2 text-sm"><span>{t('verificationCenter.documentNumber')}</span><Input aria-label={t('verificationCenter.documentNumber')} isRequired minLength={5} maxLength={64} autoComplete="off" value={documentNumber} onChange={(e) => setDocumentNumber(e.target.value)} /><span className="text-xs text-default-500">{t('verificationCenter.documentNumberHint')}</span></label>
          {reasonRequired ? <label className="grid gap-2 text-sm"><span>{t('verificationCenter.summary')} *</span><Textarea aria-label={t('verificationCenter.summary')} isRequired maxLength={512} value={summary} onChange={(e) => setSummary(e.target.value)} /></label> : null}
        </>
      ) : null}

      {step === 2 ? (
        <>
          {provider === 'alipay' ? <p className="rounded-lg border border-default-200 bg-default-50 p-3 text-xs leading-5 text-default-500">{t('verificationCenter.alipayHint')}</p> : (
            <>
              <label className="grid gap-2 text-sm"><span>{t('verificationCenter.materials')}</span><Input key={files.length === 0 ? 'empty' : 'files'} type="file" aria-label={t('verificationCenter.materials')} accept=".pdf,.png,.jpg,.jpeg,.webp" multiple onChange={(e) => { setFiles(Array.from(e.target.files ?? [])); setFileError(false) }} /><span className="text-xs text-default-500">{t('verificationCenter.fileHint')}</span></label>
              <div className="grid gap-3 md:grid-cols-2">{files.map((file, index) => <VerificationAttachment key={`${index}-${file.name}-${file.lastModified}`} name={file.name} size={file.size} contentType={file.type} file={file} />)}</div>
              {fileError ? <p role="alert" className="text-sm text-danger">{t('verificationCenter.fileError')}</p> : null}
            </>
          )}
          {mutation.isError ? <MutationError error={mutation.error} /> : null}
        </>
      ) : null}

      <div className="flex items-center justify-between gap-2 border-t border-default-200 pt-4">
        {step > 0 ? <Button type="button" variant="light" onClick={() => setStep((step - 1) as 0 | 1)}>{t('verificationCenter.back')}</Button> : <span />}
        {step < 2 ? (
          <Button type="button" color="primary" isDisabled={step === 0 ? providerOptions.length === 0 : !stepOneValid} onClick={() => setStep((step + 1) as 1 | 2)}>{t('verificationCenter.next')}</Button>
        ) : (
          <Button type="submit" color="primary" isDisabled={mutation.isPending || eligibility.isError || !providerOptions.includes(provider) || (provider !== 'alipay' && !stepTwoValid)}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" /> : <FileCheck2 className="size-4" />}{t('verificationCenter.submit')}</Button>
        )}
      </div>
    </form>
  </section>
}

function VerificationDetailPanel({ id, admin, onRestart }: { id: number; admin: boolean; onRestart: () => void }) {
  const { t } = useTranslation()
  const query = useVerificationDetail(id, admin)
  const mutation = useReviewAccountVerification()
  const [reason, setReason] = useState('')
  const review = async (status: number) => {
    if (!query.data) return
    try { await mutation.mutateAsync({ id, expected_version: query.data.case.version, status, reason: reason.trim() || undefined }); setReason('') } catch { /* Render the error beside the action. */ }
  }
  if (query.isPending) return <HistorySkeleton />
  if (!query.data) return <p role="alert" className="text-sm text-danger">{t('verificationCenter.loadError')}</p>
  return <div className="grid gap-4"><p className="whitespace-pre-wrap text-sm">{query.data.case.summary}</p>
    {!admin && query.data.case.provider === 'alipay' ? <AlipayVerification key={id} record={query.data.case} receivedAt={query.dataUpdatedAt} checking={query.isFetching} checkError={query.isError} onCheck={() => void query.refetch()} onRestart={onRestart} /> : null}
    <p className="text-xs text-default-500">{query.data.case.document_country} · {t(`verificationCenter.documentTypes.${query.data.case.document_type}`, { defaultValue: query.data.case.document_type })}{query.data.case.document_number_masked ? ` · ${query.data.case.document_number_masked}` : ''}</p>
    {query.data.case.review_reason ? <p className="rounded-lg bg-default-100 p-3 text-sm">{t('verificationCenter.reviewReason')}: {query.data.case.review_reason}</p> : null}
    <p className="text-xs text-default-500">{t('verificationCenter.updated')}: {new Date(query.data.case.updated_at * 1000).toLocaleString()}</p>
    <div className="grid gap-3 md:grid-cols-2">{query.data.materials.map((m) => <VerificationAttachment key={m.id} name={m.file_name} contentType={m.content_type} size={m.size_bytes} load={(signal) => materialBlob(`${verificationBase(admin)}/${id}/materials/${m.id}`, signal)} />)}</div>
    {admin && query.data.case.provider === 'manual' && query.data.case.status === 1 ? <div className="grid gap-3"><Textarea aria-label={t('verificationCenter.reviewReason')} maxLength={512} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t('verificationCenter.reasonHint')} /><div className="flex flex-wrap gap-2"><Button type="button" isDisabled={mutation.isPending} onClick={() => void review(4)}>{t('verificationCenter.approve')}</Button><Button type="button" variant="bordered" isDisabled={mutation.isPending || !reason.trim()} onClick={() => void review(3)}>{t('verificationCenter.supplement')}</Button><Button type="button" variant="bordered" isDisabled={mutation.isPending || !reason.trim()} onClick={() => void review(5)}>{t('verificationCenter.reject')}</Button></div>{mutation.isError ? <MutationError error={mutation.error} /> : null}</div> : null}
  </div>
}
function MutationError({ error }: { error: unknown }) {
  const { t } = useTranslation()
  const code = errorCode(error)
  const key = code === 'account_verification_self_review' ? 'verificationCenter.selfReview'
    : code === 'organization_verification_forbidden' ? 'verificationCenter.forbidden'
    : code === 'organization_verification_invalid_request' ? 'verificationCenter.invalid'
      : code === 'organization_verification_not_found' ? 'verificationCenter.notFound'
        : code === 'organization_verification_unavailable' ? 'verificationCenter.unavailable'
          : error instanceof ApiError && error.status === 409 ? 'verificationCenter.conflict' : 'verificationCenter.saveError'
  return <p role="alert" className="text-sm text-danger">{t(key)}</p>
}

function errorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const details = error.details as { code?: unknown; error?: { code?: unknown } }
  if (typeof details.code === 'string') return details.code
  return typeof details.error?.code === 'string' ? details.error.code : undefined
}
