import { useState } from 'react'
import { Button, Checkbox, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { useMerchantAction } from './merchant-pool-api'
import type { MerchantPool, MerchantPoolWrite } from './merchant-pool-model'
import { parsePoolRatio, poolRatio, positiveInteger } from './merchant-pool-model'
import { PoolError, PoolField, PoolSelect } from './merchant-pool-ui'

export function MerchantPoolAdminEditor({ pool, onClose }: { pool?: MerchantPool; onClose: () => void }) {
  const { t } = useTranslation()
  const groups = useAdminGroupCatalog()
  const action = useMerchantAction<MerchantPool>()
  const [name, setName] = useState(pool?.name ?? '')
  const [description, setDescription] = useState(pool?.description ?? '')
  const [group, setGroup] = useState(pool ? String(pool.group_id) : '')
  const [enabled, setEnabled] = useState(pool?.enabled ?? true)
  const [commission, setCommission] = useState(String((pool?.commission_bps ?? 0) / 100))
  const [inheritGlobal, setInheritGlobal] = useState(pool?.commission_inherit_global ?? true)
  const [minimum, setMinimum] = useState(poolRatio(pool?.min_ratio_micros ?? 100_000))
  const [maximum, setMaximum] = useState(poolRatio(pool?.max_ratio_micros ?? 10_000_000))
  const [settlementDelay, setSettlementDelay] = useState(String((pool?.settlement_delay_secs ?? 86400) / 3600))
  const [invalid, setInvalid] = useState(false)
  const save = async () => {
    const minRatio = parsePoolRatio(minimum), maxRatio = parsePoolRatio(maximum)
    const commissionMicros = parsePoolRatio(commission)
    const delayHours = Number(settlementDelay)
    const commissionBps = commissionMicros === undefined ? NaN : commissionMicros / 10_000
    if (!name.trim() || !positiveInteger(group) || minRatio === undefined || maxRatio === undefined || minRatio > maxRatio || !Number.isSafeInteger(commissionBps) || commissionBps < 0 || commissionBps > 10_000 || !Number.isFinite(delayHours) || delayHours < 0 || !Number.isSafeInteger(delayHours * 3600)) { setInvalid(true); return }
    const body: MerchantPoolWrite = { name: name.trim(), description: description.trim(), group_id: Number(group), enabled, commission_bps: commissionBps, commission_inherit_global: inheritGlobal, min_ratio_micros: minRatio, max_ratio_micros: maxRatio, settlement_delay_secs: delayHours * 3600 }
    try { await action.mutateAsync({ url: pool ? `/api/admin/merchant-pools/${pool.id}` : '/api/admin/merchant-pools', method: pool ? 'put' : 'post', body }); onClose() } catch { /* A conflicting configuration remains visible for correction. */ }
  }
  return <Drawer isOpen placement="right" size="xl" onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><DrawerContent><DrawerHeader>{t(pool ? 'merchantPool.editPool' : 'merchantPool.createPool')}</DrawerHeader><DrawerBody className="gap-4 pb-5"><PoolField label={t('merchantPool.name')}><Input size="sm" aria-label={t('merchantPool.name')} value={name} onValueChange={setName} maxLength={128} /></PoolField><PoolField label={t('merchantPool.description')}><Textarea size="sm" aria-label={t('merchantPool.description')} value={description} onValueChange={setDescription} maxLength={2000} /></PoolField><PoolField label={t('merchantPool.group')} hint={t('merchantPool.poolGroupHint')}><PoolSelect label={t('merchantPool.group')} value={group} disabled={Boolean(pool) || groups.isFetching} onChange={setGroup} options={(groups.data ?? []).map((item) => ({ value: String(item.id), label: item.name }))} /></PoolField>{groups.isError ? <PoolError onRetry={() => void groups.refetch()} /> : null}<Checkbox size="sm" isSelected={inheritGlobal} onValueChange={setInheritGlobal}>{t('merchantPool.inheritGlobalCommission')}</Checkbox><PoolField label={t('merchantPool.commissionRate')}><Input size="sm" aria-label={t('merchantPool.commissionRate')} inputMode="decimal" isDisabled={inheritGlobal} value={commission} onValueChange={setCommission} endContent="%" /></PoolField><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.minimumRatio')}><Input size="sm" aria-label={t('merchantPool.minimumRatio')} inputMode="decimal" value={minimum} onValueChange={setMinimum} /></PoolField><PoolField label={t('merchantPool.maximumRatio')}><Input size="sm" aria-label={t('merchantPool.maximumRatio')} inputMode="decimal" value={maximum} onValueChange={setMaximum} /></PoolField></div><PoolField label={t('merchantPool.settlementDelay')}><Input size="sm" aria-label={t('merchantPool.settlementDelay')} type="number" min="0" step="1" value={settlementDelay} onValueChange={setSettlementDelay} /></PoolField><Checkbox size="sm" isSelected={enabled} onValueChange={setEnabled}>{t('merchantPool.enabled')}</Checkbox>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button><Button size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></div></DrawerBody></DrawerContent></Drawer>
}
