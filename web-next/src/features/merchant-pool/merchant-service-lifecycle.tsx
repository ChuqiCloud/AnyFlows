import { useEffect, useState } from 'react'
import { Checkbox, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Input, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction } from './merchant-pool-api'
import { merchantServicePath, useMerchantMaintenance, useMerchantServicePrices } from './merchant-extension-api'
import { merchantDateTimeSeconds, merchantLocalDateTime, type MerchantMaintenance, type MerchantServicePriceWrite } from './merchant-extension-model'
import { poolRatio, poolTimestamp, parsePoolRatio, type MerchantOwnedService } from './merchant-pool-model'
import { MerchantManagementTable } from './merchant-management-tables'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolPager, PoolStatus } from './merchant-pool-ui'

const cell = 'border-b border-[var(--hairline)] px-2 py-2 align-middle whitespace-nowrap'

export function MerchantServiceLifecycle({ service, admin = false, onClose }: { service: MerchantOwnedService; admin?: boolean; onClose: () => void }) {
  const { t } = useTranslation()
  const [pending, setPending] = useState(false)
  const [panel, setPanel] = useState<'prices' | 'maintenance'>('prices')
  return <Drawer isOpen isDismissable={!pending} isKeyboardDismissDisabled={pending} size="4xl" placement="right" scrollBehavior="inside" onOpenChange={(open) => { if (!open && !pending) onClose() }}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader className="flex-col gap-1"><h2>{t('merchantPool.serviceLifecycle')}: {service.name}</h2><p className="text-xs font-normal text-muted-foreground">{t('merchantPool.serviceLifecycleHint')}</p></DrawerHeader><DrawerBody className="gap-3 pb-5"><div className="flex gap-1" role="group" aria-label={t('merchantPool.serviceLifecycle')}>{(['prices', 'maintenance'] as const).map((value) => <Button key={value} size="sm" isDisabled={pending} variant={panel === value ? 'flat' : 'light'} onPress={() => setPanel(value)}>{t(`merchantPool.lifecycleTabs.${value}`)}</Button>)}</div>{panel === 'prices' ? <MerchantServicePrices service={service} admin={admin} onPendingChange={setPending} /> : <MerchantMaintenancePanel serviceId={service.id} admin={admin} onPendingChange={setPending} />}</DrawerBody><DrawerFooter><Button size="sm" variant="flat" isDisabled={pending} onPress={onClose}>{t('merchantPool.close')}</Button></DrawerFooter></DrawerContent></Drawer>
}

function MerchantServicePrices({ service, admin, onPendingChange }: { service: MerchantOwnedService; admin: boolean; onPendingChange: (pending: boolean) => void }) {
  const { t, i18n } = useTranslation()
  const [page, setPage] = useState(0)
  const query = useMerchantServicePrices(service.id, admin, page + 1)
  const action = useMerchantAction()
  useEffect(() => { onPendingChange(action.isPending) }, [action.isPending, onPendingChange])
  const published = service.published
  const active = service.active_price
  const [immediate, setImmediate] = useState(true)
  const [effective, setEffective] = useState(merchantLocalDateTime(Math.floor(Date.now() / 1000) + 3600))
  const [ratio, setRatio] = useState(poolRatio(active?.default_ratio_micros ?? published?.default_ratio_micros ?? 1_000_000))
  const [models, setModels] = useState(() => published?.models.map((model) => ({ model: model.model, ratio: (() => { const existing = active ? active.model_ratios.find((item) => item.model === model.model)?.ratio_micros : model.ratio_micros; return existing == null ? '' : poolRatio(existing) })() })) ?? [])
  const [invalid, setInvalid] = useState(false)
  const save = async () => {
    const defaultRatio = parsePoolRatio(ratio)
    const at = immediate ? Math.floor(Date.now() / 1000) : merchantDateTimeSeconds(effective)
    if (!published || defaultRatio === undefined || defaultRatio <= 0 || at === undefined || at < Math.floor(Date.now() / 1000) - 5 || at > Math.floor(Date.now() / 1000) + 90 * 86400 || defaultRatio > 100_000_000 || models.some((item) => item.ratio !== '' && ((parsePoolRatio(item.ratio) ?? 0) <= 0 || (parsePoolRatio(item.ratio) ?? 0) > 100_000_000))) { setInvalid(true); return }
    const body: MerchantServicePriceWrite = { expected_revision_id: published.id, effective_at: at, default_ratio_micros: defaultRatio, model_ratios: models.map((item) => ({ model: item.model, ratio_micros: item.ratio === '' ? null : parsePoolRatio(item.ratio) ?? null })) }
    try { await action.mutateAsync({ url: `${merchantServicePath(service.id, admin)}/prices`, body }); setInvalid(false) } catch { /* Keep exact quote inputs and expected revision on failure. */ }
  }
  const cancel = async (id: number) => { try { await action.mutateAsync({ url: `${merchantServicePath(service.id, admin)}/prices/${id}/cancel`, body: {} }) } catch { /* A version that became active cannot be cancelled. */ } }
  return <fieldset disabled={action.isPending} className="grid min-w-0 gap-3">{published ? <><p className="text-xs text-muted-foreground">{t('merchantPool.priceScheduleHint')}</p><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.defaultRatio')}><Input aria-label={t('merchantPool.defaultRatio')} size="sm" inputMode="decimal" value={ratio} onValueChange={setRatio} /></PoolField><PoolField label={t('merchantPool.effectiveAt')}><Input aria-label={t('merchantPool.effectiveAt')} size="sm" type="datetime-local" isDisabled={immediate} value={effective} onValueChange={setEffective} /></PoolField></div><Checkbox size="sm" isSelected={immediate} onValueChange={setImmediate}>{t('merchantPool.effectiveNow')}</Checkbox><MerchantManagementTable label={t('merchantPool.models')} columns={['model', 'ratio']}>{models.map((item) => <tr key={item.model}><td className={`${cell} font-mono`}>{item.model}</td><td className={cell}><Input aria-label={t('merchantPool.modelRatioNamed', { model: item.model })} className="max-w-44" size="sm" inputMode="decimal" value={item.ratio} placeholder={ratio} onValueChange={(value) => setModels((current) => current.map((model) => model.model === item.model ? { ...model, ratio: value } : model))} /></td></tr>)}</MerchantManagementTable><Button className="justify-self-end" size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.publishPrice')}</Button></> : <PoolEmpty>{t('merchantPool.publishBeforePricing')}</PoolEmpty>}{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}{query.isError ? <PoolError onRetry={() => void query.refetch()} /> : query.data?.items.length ? <MerchantManagementTable label={t('merchantPool.priceHistory')} columns={['reference', 'revision', 'ratio', 'effective', 'status', 'reason', 'actions']}>{query.data.items.map((price) => <tr key={price.id}><td className={cell}>#{price.id}</td><td className={cell}>#{price.revision_id}</td><td className={cell}>{poolRatio(price.default_ratio_micros)}×</td><td className={cell}>{price.effective_at === 0 ? t('merchantPool.initialPrice') : poolTimestamp(price.effective_at, i18n.language)}</td><td className={cell}><PoolStatus status={price.status} /></td><td className={cell}><span className="block max-w-48 truncate" title={price.reason ?? undefined}>{price.reason || '—'}</span></td><td className={cell}>{price.status === 'scheduled' ? <Button size="sm" variant="light" isDisabled={action.isPending} onPress={() => void cancel(price.id)}>{t('merchantPool.cancelSchedule')}</Button> : '—'}</td></tr>)}</MerchantManagementTable> : <PoolEmpty />}{query.data ? <PoolPager page={page} hasMore={(page + 1) * query.data.page_size < query.data.total} loading={query.isFetching} onChange={setPage} /> : null}</fieldset>
}

function MerchantMaintenancePanel({ serviceId, admin, onPendingChange }: { serviceId: number; admin: boolean; onPendingChange: (pending: boolean) => void }) {
  const { t, i18n } = useTranslation()
  const [page, setPage] = useState(0)
  const query = useMerchantMaintenance(serviceId, admin, page + 1)
  const action = useMerchantAction()
  useEffect(() => { onPendingChange(action.isPending) }, [action.isPending, onPendingChange])
  const [editing, setEditing] = useState<MerchantMaintenance>()
  const [message, setMessage] = useState('')
  const [starts, setStarts] = useState(merchantLocalDateTime(Math.floor(Date.now() / 1000)))
  const [ends, setEnds] = useState(merchantLocalDateTime(Math.floor(Date.now() / 1000) + 3600))
  const [blocking, setBlocking] = useState(false)
  const [invalid, setInvalid] = useState(false)
  const edit = (notice?: MerchantMaintenance) => { setEditing(notice); setMessage(notice?.message ?? ''); setBlocking(notice?.blocks_requests ?? false); setStarts(merchantLocalDateTime(notice?.starts_at ?? Math.floor(Date.now() / 1000))); setEnds(merchantLocalDateTime(notice?.ends_at ?? Math.floor(Date.now() / 1000) + 3600)) }
  const save = async () => {
    const start = merchantDateTimeSeconds(starts), end = merchantDateTimeSeconds(ends)
    if (!message.trim() || start === undefined || end === undefined || end <= start || end <= Date.now() / 1000 || end - start > 30 * 86400 || end > Date.now() / 1000 + 90 * 86400) { setInvalid(true); return }
    try { await action.mutateAsync({ url: `${merchantServicePath(serviceId, admin)}/maintenance${editing ? `/${editing.id}` : ''}`, method: editing ? 'put' : 'post', body: { starts_at: start, ends_at: end, message: message.trim(), blocks_requests: blocking } }); setInvalid(false); edit() } catch { /* Keep the notice and its time interval for correction. */ }
  }
  const cancel = async (id: number) => { try { await action.mutateAsync({ url: `${merchantServicePath(serviceId, admin)}/maintenance/${id}/cancel`, body: {} }) } catch { /* Do not report cancellation until the server confirms it. */ } }
  return <fieldset disabled={action.isPending} className="grid min-w-0 gap-3"><PoolField label={t('merchantPool.maintenanceMessage')}><Textarea aria-label={t('merchantPool.maintenanceMessage')} size="sm" value={message} onValueChange={setMessage} maxLength={2000} /></PoolField><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.startsAt')}><Input aria-label={t('merchantPool.startsAt')} size="sm" type="datetime-local" value={starts} onValueChange={setStarts} /></PoolField><PoolField label={t('merchantPool.endsAt')}><Input aria-label={t('merchantPool.endsAt')} size="sm" type="datetime-local" value={ends} onValueChange={setEnds} /></PoolField></div><Checkbox size="sm" isSelected={blocking} onValueChange={setBlocking}>{t('merchantPool.maintenanceBlocking')}</Checkbox><p className="text-xs text-muted-foreground">{t('merchantPool.maintenanceHint')}</p><div className="flex justify-end gap-2">{editing ? <Button size="sm" variant="flat" isDisabled={action.isPending} onPress={() => edit()}>{t('merchantPool.cancel')}</Button> : null}<Button size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t(editing ? 'merchantPool.save' : 'merchantPool.publishNotice')}</Button></div>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}{query.isError ? <PoolError onRetry={() => void query.refetch()} /> : query.data?.items.length ? <MerchantManagementTable label={t('merchantPool.lifecycleTabs.maintenance')} columns={['description', 'starts', 'ends', 'status', 'blocking', 'actions']}>{query.data.items.map((notice) => <tr key={notice.id}><td className={cell}><span className="block max-w-72 truncate" title={notice.message}>{notice.message}</span></td><td className={cell}>{poolTimestamp(notice.starts_at, i18n.language)}</td><td className={cell}>{poolTimestamp(notice.ends_at, i18n.language)}</td><td className={cell}><PoolStatus status={notice.status} /></td><td className={cell}>{t(notice.blocks_requests ? 'merchantPool.yes' : 'merchantPool.no')}</td><td className={cell}>{['scheduled', 'active'].includes(notice.status) ? <div className="flex gap-1"><Button size="sm" variant="light" onPress={() => edit(notice)}>{t('merchantPool.edit')}</Button><Button size="sm" variant="light" isDisabled={action.isPending} onPress={() => void cancel(notice.id)}>{t('merchantPool.cancelNotice')}</Button></div> : '—'}</td></tr>)}</MerchantManagementTable> : <PoolEmpty />}{query.data ? <PoolPager page={page} hasMore={(page + 1) * query.data.page_size < query.data.total} loading={query.isFetching} onChange={setPage} /> : null}</fieldset>
}
