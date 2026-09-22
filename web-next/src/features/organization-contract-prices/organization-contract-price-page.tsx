import { Button, Chip, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem, Skeleton } from '@heroui/react'
import { AlertTriangle, Ban, CircleDollarSign, LoaderCircle, Plus, RefreshCw, ShieldX } from 'lucide-react'
import { useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import type { ContractPriceProtocol, ContractPriceResponse, ContractPriceValues, OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { organizationContractPriceErrorCode, organizationContractPriceErrorStatus, useCloseOrganizationContractPrice, useCreateOrganizationContractPrice, useOrganizationContractPrices } from './organization-contract-price-api'

const CONTRACT_PRICES_READ = 'organization.contract_prices.read'
const CONTRACT_PRICES_MANAGE = 'organization.contract_prices.manage'
const PROTOCOLS: ContractPriceProtocol[] = ['openai_chat', 'openai_responses', 'openai_embeddings', 'openai_images', 'openai_audio', 'openai_speech', 'jina_rerank', 'cohere_rerank', 'xai_video', 'anthropic', 'gemini']
const PRICE_FIELDS: Array<keyof ContractPriceValues> = ['input', 'output', 'cache_read', 'cache_creation_5m', 'cache_creation_1h']

/** 企业合同价管理页；权限、区间冲突和版本 CAS 均以服务端事实为准。 */
export function OrganizationContractPricePage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={CircleDollarSign} title={t('organizationContractPrices.title')} subtitle={t('organizationContractPrices.subtitle')} />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => <OrganizationContractPriceWorkspace key={organizationId} organizationId={organizationId} overview={overview} />}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationContractPriceWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { i18n, t } = useTranslation()
  const canRead = overview.permissions.includes(CONTRACT_PRICES_READ)
  const canManage = overview.permissions.includes(CONTRACT_PRICES_MANAGE)
  const query = useOrganizationContractPrices(organizationId, canRead)
  const createMutation = useCreateOrganizationContractPrice(organizationId)
  const closeMutation = useCloseOrganizationContractPrice(organizationId)
  const [createOpen, setCreateOpen] = useState(false)
  const [closeTarget, setCloseTarget] = useState<ContractPriceResponse>()
  const prices = query.data?.prices ?? []
  const formatTime = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])

  if (!canRead) return <OrganizationPageState icon={ShieldX} title={t('organizationContractPrices.restricted.title')} body={t('organizationContractPrices.restricted.body')} />

  const refresh = async () => {
    await query.refetch()
    createMutation.reset()
    closeMutation.reset()
  }
  const close = async () => {
    if (!closeTarget || closeTarget.effective_from > Math.floor(Date.now() / 1000)) return
    try {
      await closeMutation.mutateAsync({
        priceId: closeTarget.id,
        body: { effective_until: Math.floor(Date.now() / 1000), expected_version: closeTarget.version },
      })
      setCloseTarget(undefined)
    } catch {
      // 冲突时保留目标，用户刷新后按最新版本再次确认。
    }
  }
  const errorCode = organizationContractPriceErrorCode(query.error ?? createMutation.error ?? closeMutation.error)
  const errorStatus = organizationContractPriceErrorStatus(query.error ?? createMutation.error ?? closeMutation.error)

  return (
    <div className="grid min-w-0 gap-4">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] py-3">
        <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
          <CircleDollarSign className="size-3.5 shrink-0 text-brand" aria-hidden="true" />
          <span>{t('organizationContractPrices.summary.count', { count: prices.length })}</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => void refresh()}><RefreshCw className={query.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('organizationContractPrices.actions.refresh')}</Button>
          <Button type="button" size="sm" color="primary" isDisabled={!canManage || query.isPending} onClick={() => { createMutation.reset(); setCreateOpen(true) }}><Plus className="size-3.5" aria-hidden="true" />{t('organizationContractPrices.actions.create')}</Button>
        </div>
      </div>

      {!canManage ? <p className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning"><ShieldX className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('organizationContractPrices.readOnly')}</p> : null}

      {query.isPending ? <ContractPriceLoading /> : query.isError ? (
        <OrganizationPageState icon={errorStatus === 403 ? ShieldX : AlertTriangle} title={t(errorStatus === 403 ? 'organizationContractPrices.errors.forbiddenTitle' : 'organizationContractPrices.errors.listTitle')} body={t(errorStatus === 403 ? 'organizationContractPrices.errors.forbiddenBody' : 'organizationContractPrices.errors.listBody')} action={<Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationContractPrices.actions.retry')}</Button>} />
      ) : prices.length === 0 ? (
        <OrganizationPageState icon={CircleDollarSign} title={t('organizationContractPrices.empty.title')} body={t('organizationContractPrices.empty.body')} />
      ) : (
        <ContractPriceList prices={prices} canManage={canManage} formatTime={formatTime} onClose={setCloseTarget} />
      )}

      {errorCode && !query.isError ? <MutationError code={errorCode} onRefresh={() => void refresh()} /> : null}
      <ContractPriceCreateForm open={createOpen} onOpenChange={setCreateOpen} prices={prices} mutation={createMutation} />
      <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={closeTarget !== undefined} onOpenChange={(open) => { if (!open && !closeMutation.isPending) { closeMutation.reset(); setCloseTarget(undefined) } }}>
        <ModalContent>
          {() => (
            <>
              <ModalHeader className="grid gap-1.5"><h2 className="text-base font-semibold">{t('organizationContractPrices.close.title')}</h2><p className="text-sm leading-5 font-normal text-muted-foreground">{t('organizationContractPrices.close.description', { model: closeTarget?.model, protocol: closeTarget ? t(`organizationContractPrices.protocol.${closeTarget.protocol}`) : '' })}</p></ModalHeader>
              <ModalBody className="gap-1.5">
                {closeMutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs font-normal text-destructive">{t(organizationContractPriceErrorStatus(closeMutation.error) === 409 ? 'organizationContractPrices.errors.conflict' : 'organizationContractPrices.errors.mutation')}</p> : null}
              </ModalBody>
              <ModalFooter>
                <Button variant="light" isDisabled={closeMutation.isPending} onPress={() => setCloseTarget(undefined)}>{t('organizationContractPrices.actions.cancel')}</Button>
                <Button color="danger" variant="flat" isDisabled={closeMutation.isPending} onPress={() => void close()}>{closeMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Ban className="size-4" aria-hidden="true" />}{t('organizationContractPrices.actions.close')}</Button>
              </ModalFooter>
            </>
          )}
        </ModalContent>
      </Modal>
    </div>
  )
}

function ContractPriceList({ prices, canManage, formatTime, onClose }: { prices: ContractPriceResponse[]; canManage: boolean; formatTime: Intl.DateTimeFormat; onClose: (price: ContractPriceResponse) => void }) {
  const { t } = useTranslation()
  const now = Math.floor(Date.now() / 1000)
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 lg:block">
        <table className="w-full table-fixed text-left text-xs"><thead className="bg-surface-2/60 text-muted-foreground"><tr><th className="w-[19%] px-3 py-2 font-medium">{t('organizationContractPrices.columns.model')}</th><th className="w-[14%] px-3 py-2 font-medium">{t('organizationContractPrices.columns.protocol')}</th><th className="w-[23%] px-3 py-2 font-medium">{t('organizationContractPrices.columns.prices')}</th><th className="w-[18%] px-3 py-2 font-medium">{t('organizationContractPrices.columns.effective')}</th><th className="w-[12%] px-3 py-2 font-medium">{t('organizationContractPrices.columns.version')}</th><th className="w-[14%] px-3 py-2"><span className="sr-only">{t('organizationContractPrices.columns.actions')}</span></th></tr></thead><tbody className="divide-y divide-[var(--hairline)]">{prices.map((price) => { const current = price.effective_from <= now; return <tr key={price.id} className="hover:bg-surface-2/35"><td className="truncate px-3 py-2.5 font-medium" title={price.model}>{price.model}</td><td className="px-3 py-2.5"><Chip size="sm" variant="flat">{t(`organizationContractPrices.protocol.${price.protocol}`)}</Chip></td><td className="px-3 py-2.5 font-mono text-[0.6875rem]">{price.prices.input} / {price.prices.output}</td><td className="px-3 py-2.5 text-muted-foreground">{formatTime.format(price.effective_from * 1000)}{price.effective_until ? ` – ${formatTime.format(price.effective_until * 1000)}` : ` · ${t(current ? 'organizationContractPrices.current' : 'organizationContractPrices.scheduled')}`}</td><td className="px-3 py-2.5 text-muted-foreground">v{price.version}</td><td className="px-2 py-2.5">{canManage && price.effective_until === null && current ? <Button isIconOnly type="button" size="sm" variant="light" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationContractPrices.actions.close')} onClick={() => onClose(price)}><Ban className="size-3.5" aria-hidden="true" /></Button> : null}</td></tr> })}</tbody></table>
      </div>
      <div className="grid gap-2 lg:hidden">{prices.map((price) => { const current = price.effective_from <= now; return <article key={price.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex min-w-0 flex-wrap items-center gap-2"><span className="truncate text-sm font-medium">{price.model}</span><Chip size="sm" variant="flat">{t(`organizationContractPrices.protocol.${price.protocol}`)}</Chip></div><p className="mt-1 font-mono text-[0.6875rem] text-muted-foreground">{t('organizationContractPrices.pricePair', { input: price.prices.input, output: price.prices.output })}</p></div>{canManage && price.effective_until === null && current ? <Button isIconOnly type="button" size="sm" variant="light" className="text-muted-foreground hover:text-destructive" aria-label={t('organizationContractPrices.actions.close')} onClick={() => onClose(price)}><Ban className="size-3.5" aria-hidden="true" /></Button> : null}</div><dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs"><div><dt className="text-muted-foreground">{t('organizationContractPrices.columns.effective')}</dt><dd className="mt-1 text-muted-foreground">{formatTime.format(price.effective_from * 1000)}</dd></div><div><dt className="text-muted-foreground">{t('organizationContractPrices.columns.version')}</dt><dd className="mt-1 font-mono">v{price.version}</dd></div></dl></article> })}</div>
    </>
  )
}

function ContractPriceCreateForm({ open, onOpenChange, prices, mutation }: { open: boolean; onOpenChange: (open: boolean) => void; prices: ContractPriceResponse[]; mutation: ReturnType<typeof useCreateOrganizationContractPrice> }) {
  const { t } = useTranslation()
  const [model, setModel] = useState('')
  const [protocol, setProtocol] = useState<ContractPriceProtocol>('openai_chat')
  const [values, setValues] = useState<ContractPriceValues>({ input: '', output: '', cache_read: '', cache_creation_5m: '', cache_creation_1h: '' })
  const [effectiveFrom, setEffectiveFrom] = useState('')
  const [effectiveUntil, setEffectiveUntil] = useState('')
  const protocolItems = PROTOCOLS.map((item) => ({ key: item, label: t(`organizationContractPrices.protocol.${item}`) }))
  const decimalPattern = /^(0|[1-9]\d{0,9})(\.\d{1,28})?$/
  const validPrices = PRICE_FIELDS.every((field) => decimalPattern.test(values[field]))
  const validDates = Boolean(effectiveFrom) && (!effectiveUntil || new Date(effectiveUntil).getTime() > new Date(effectiveFrom).getTime())
  const valid = model.trim().length > 0 && model === model.trim() && model.length <= 256 && validPrices && validDates
  const latestVersion = prices.filter((price) => price.model === model.trim() && price.protocol === protocol).reduce((version, price) => Math.max(version, price.version), 0)
  const reset = () => { setModel(''); setProtocol('openai_chat'); setValues({ input: '', output: '', cache_read: '', cache_creation_5m: '', cache_creation_1h: '' }); setEffectiveFrom(''); setEffectiveUntil(''); mutation.reset() }
  const close = (nextOpen: boolean) => { onOpenChange(nextOpen); if (!nextOpen && !mutation.isPending) reset() }
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || mutation.isPending) return
    try {
      await mutation.mutateAsync({ model: model.trim(), protocol, prices: values, effective_from: Math.floor(new Date(effectiveFrom).getTime() / 1000), effective_until: effectiveUntil ? Math.floor(new Date(effectiveUntil).getTime() / 1000) : null, expected_version: latestVersion || null })
      close(false)
    } catch {
      // 区间或版本冲突时保留输入，用户可刷新列表后修正生效时间。
    }
  }
  return <Modal backdrop="blur" hideCloseButton isDismissable={false} isOpen={open} scrollBehavior="inside" size="2xl" onOpenChange={close}><ModalContent>{() => (<><ModalHeader className="grid gap-1.5"><h2 className="text-base font-semibold">{t('organizationContractPrices.form.title')}</h2><p className="text-sm leading-5 font-normal text-muted-foreground">{t('organizationContractPrices.form.subtitle')}</p></ModalHeader><form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate><ModalBody className="grid gap-4"><div className="grid gap-3 sm:grid-cols-2"><Field id="contract-price-model" label={t('organizationContractPrices.form.model')}><Input id="contract-price-model" isDisabled={mutation.isPending} maxLength={256} size="sm" value={model} onChange={(event) => { setModel(event.target.value); mutation.reset() }} /></Field><Field id="contract-price-protocol" label={t('organizationContractPrices.form.protocol')}><Select aria-label={t('organizationContractPrices.form.protocol')} id="contract-price-protocol" isDisabled={mutation.isPending} items={protocolItems} selectedKeys={[protocol]} size="sm" onSelectionChange={(keys) => { setProtocol(String(Array.from(keys)[0] ?? 'openai_chat') as ContractPriceProtocol); mutation.reset() }}>{(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}</Select></Field></div><div className="grid gap-3 sm:grid-cols-2">{PRICE_FIELDS.map((field) => <Field key={field} id={`contract-price-${field}`} label={t(`organizationContractPrices.form.${field}`)}><Input id={`contract-price-${field}`} inputMode="decimal" isDisabled={mutation.isPending} size="sm" value={values[field]} onChange={(event) => { setValues((current) => ({ ...current, [field]: event.target.value })); mutation.reset() }} /></Field>)}</div><div className="grid gap-3 sm:grid-cols-2"><Field id="contract-price-effective-from" label={t('organizationContractPrices.form.effectiveFrom')}><Input id="contract-price-effective-from" isDisabled={mutation.isPending} size="sm" step={60} type="datetime-local" value={effectiveFrom} onChange={(event) => setEffectiveFrom(event.target.value)} /></Field><Field id="contract-price-effective-until" label={t('organizationContractPrices.form.effectiveUntil')} hint={t('organizationContractPrices.form.effectiveUntilHint')}><Input id="contract-price-effective-until" isDisabled={mutation.isPending} size="sm" step={60} type="datetime-local" value={effectiveUntil} onChange={(event) => setEffectiveUntil(event.target.value)} /></Field></div>{latestVersion ? <p className="text-[0.6875rem] text-muted-foreground">{t('organizationContractPrices.form.expectedVersion', { version: latestVersion })}</p> : null}{mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(organizationContractPriceErrorStatus(mutation.error) === 409 ? (organizationContractPriceErrorCode(mutation.error) === 'organization_contract_price_interval_conflict' ? 'organizationContractPrices.errors.intervalConflict' : 'organizationContractPrices.errors.conflict') : 'organizationContractPrices.errors.mutation')}</p> : null}</ModalBody><ModalFooter><Button variant="light" isDisabled={mutation.isPending} onPress={() => close(false)}>{t('organizationContractPrices.actions.cancel')}</Button><Button type="submit" color="primary" isDisabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Plus className="size-4" aria-hidden="true" />}{t(mutation.isPending ? 'organizationContractPrices.actions.creating' : 'organizationContractPrices.actions.create')}</Button></ModalFooter></form></>)}</ModalContent></Modal>
}

function Field({ id, label, hint, children }: { id: string; label: string; hint?: string; children: ReactNode }) { return <div className="grid gap-1.5"><label htmlFor={id} className="text-xs font-medium">{label}</label>{children}{hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}</div> }
function MutationError({ code, onRefresh }: { code: string; onRefresh: () => void }) { const { t } = useTranslation(); return <div role="alert" className="flex items-center justify-between gap-3 rounded-lg border border-destructive/25 bg-destructive/8 px-3 py-2 text-xs text-destructive"><span>{t(code === 'organization_contract_price_interval_conflict' ? 'organizationContractPrices.errors.intervalConflict' : code === 'organization_contract_price_conflict' ? 'organizationContractPrices.errors.conflict' : 'organizationContractPrices.errors.mutation')}</span><Button type="button" size="sm" variant="bordered" onClick={onRefresh}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationContractPrices.actions.verify')}</Button></div> }
function ContractPriceLoading() { return <div className="grid gap-2" role="status">{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-16 rounded-lg" />)}</div> }
