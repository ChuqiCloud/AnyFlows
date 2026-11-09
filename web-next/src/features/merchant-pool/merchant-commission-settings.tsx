import { useState } from 'react'
import { Checkbox, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction, useMerchantSettings, useMerchantTerms } from './merchant-pool-api'
import { parsePoolRatio, positiveInteger, type MerchantPool } from './merchant-pool-model'
import { PoolButton as Button, PoolError, PoolField, PoolLoading, PoolSelect } from './merchant-pool-ui'

function commissionBps(value: string) { const micros = parsePoolRatio(value); if (micros === undefined || micros % 10_000 !== 0 || micros > 100_000_000) return undefined; return micros / 10_000 }

export function MerchantCommissionSettings() {
  const { t } = useTranslation()
  const settings = useMerchantSettings()
  return <section className="rounded-xl border border-[var(--hairline)] p-4"><h3 className="mb-3 text-sm font-semibold">{t('merchantPool.globalCommission')}</h3>{settings.isPending ? <PoolLoading /> : settings.isError ? <PoolError onRetry={() => void settings.refetch()} /> : <CommissionForm initial={settings.data.commission_bps} />}</section>
}

function CommissionForm({ initial }: { initial: number }) {
  const { t } = useTranslation()
  const [value, setValue] = useState(String(initial / 100))
  const [invalid, setInvalid] = useState(false)
  const [saved, setSaved] = useState(false)
  const action = useMerchantAction()
  const save = async () => { const bps = commissionBps(value); if (bps === undefined) { setInvalid(true); return }; try { await action.mutateAsync({ url: '/api/admin/merchant-pool-settings', method: 'put', body: { commission_bps: bps } }); setSaved(true); setInvalid(false) } catch { /* Preserve the administrator's input. */ } }
  return <div className="grid gap-3"><p className="text-xs text-muted-foreground">{t('merchantPool.commissionPrecedence')}</p><div className="flex items-end gap-3"><PoolField label={t('merchantPool.commissionRate')}><Input size="sm" inputMode="decimal" aria-label={t('merchantPool.commissionRate')} value={value} onValueChange={(next) => { setValue(next); setSaved(false) }} endContent="%" /></PoolField><Button size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></div>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}{saved ? <p role="status" className="text-xs text-success">{t('merchantPool.saved')}</p> : null}</div>
}

export function MerchantTermsEditor({ merchantId, pools, onClose }: { merchantId: number; pools: MerchantPool[]; onClose: () => void }) {
  const { t } = useTranslation()
  const [pool, setPool] = useState(String(pools[0]?.id ?? ''))
  const terms = useMerchantTerms(positiveInteger(pool), merchantId)
  return <Modal isOpen onOpenChange={(open) => { if (!open) onClose() }}><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t('merchantPool.merchantTerms')}</ModalHeader><ModalBody className="pb-5"><PoolField label={t('merchantPool.pool')}><PoolSelect label={t('merchantPool.pool')} allowEmpty={false} value={pool} onChange={setPool} options={pools.map((item) => ({ value: String(item.id), label: item.name }))} /></PoolField>{!pool ? <p className="text-xs">{t('merchantPool.noSupplyPool')}</p> : terms.isPending ? <PoolLoading /> : terms.isError ? <PoolError onRetry={() => void terms.refetch()} /> : <TermsForm key={pool} poolId={Number(pool)} merchantId={merchantId} initial={terms.data.commission_bps} onSaved={onClose} />}</ModalBody></ModalContent></Modal>
}

function TermsForm({ poolId, merchantId, initial, onSaved }: { poolId: number; merchantId: number; initial: number | null; onSaved: () => void }) {
  const { t } = useTranslation()
  const [inherit, setInherit] = useState(initial === null)
  const [value, setValue] = useState(String((initial ?? 0) / 100))
  const [invalid, setInvalid] = useState(false)
  const action = useMerchantAction()
  const save = async () => { const bps = inherit ? null : commissionBps(value); if (bps === undefined) { setInvalid(true); return }; try { await action.mutateAsync({ url: `/api/admin/merchant-pools/${poolId}/merchants/${merchantId}/terms`, method: 'put', body: { commission_bps: bps } }); onSaved() } catch { /* Keep terms open after a failed write. */ } }
  return <div className="grid gap-3"><Checkbox size="sm" isSelected={inherit} onValueChange={setInherit}>{t('merchantPool.inheritPoolCommission')}</Checkbox><PoolField label={t('merchantPool.commissionRate')}><Input size="sm" aria-label={t('merchantPool.commissionRate')} inputMode="decimal" isDisabled={inherit} value={value} onValueChange={setValue} endContent="%" /></PoolField>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<Button className="justify-self-end" size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></div>
}

export function MerchantSettlementReversal({ initialReference = '', onClose }: { initialReference?: string; onClose: () => void }) {
  const { t } = useTranslation()
  const [reference, setReference] = useState(initialReference)
  const [reason, setReason] = useState('')
  const [invalid, setInvalid] = useState(false)
  const action = useMerchantAction()
  const reverse = async () => { if (!/^[0-9a-f]{32}$/.test(reference) || !reason.trim()) { setInvalid(true); return }; try { await action.mutateAsync({ url: `/api/admin/merchant-settlements/${reference}/reverse`, body: { reason: reason.trim() } }); onClose() } catch { /* The original transaction remains available for reconciliation. */ } }
  return <Modal isOpen onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t('merchantPool.reverseSettlement')}</ModalHeader><ModalBody><p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.reverseHint')}</p><PoolField label={t('merchantPool.settlementReference')}><Input size="sm" aria-label={t('merchantPool.settlementReference')} value={reference} onValueChange={setReference} autoComplete="off" /></PoolField><PoolField label={t('merchantPool.reviewReason')}><Textarea size="sm" aria-label={t('merchantPool.reviewReason')} value={reason} onValueChange={setReason} maxLength={500} /></PoolField>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}</ModalBody><ModalFooter><Button size="sm" variant="flat" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button><Button size="sm" color="danger" isLoading={action.isPending} onPress={() => void reverse()}>{t('merchantPool.confirm')}</Button></ModalFooter></ModalContent></Modal>
}
