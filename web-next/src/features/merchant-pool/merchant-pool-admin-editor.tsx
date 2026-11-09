import { useState, type Dispatch, type SetStateAction } from 'react'
import { Checkbox, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { ApiError } from '@/lib/api'
import { merchantDescriptionError, merchantTextError } from './merchant-form-validation'
import { useMerchantAction } from './merchant-pool-api'
import type { MerchantPool, MerchantPoolWrite } from './merchant-pool-model'
import { parsePoolRatio, poolRatio, positiveInteger } from './merchant-pool-model'
import { PoolButton as Button, PoolError, PoolField, PoolSelect } from './merchant-pool-ui'

type PoolFieldName = 'name' | 'description' | 'group' | 'commission' | 'minimum' | 'maximum' | 'settlementDelay'
type PoolFieldErrors = Partial<Record<PoolFieldName, string>>

function delaySeconds(value: string): number | undefined {
  if (!/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value) || value.length > 32) return undefined
  const [hours, fraction = ''] = value.split('.')
  const scale = 10n ** BigInt(fraction.length)
  const seconds = (BigInt(hours) * scale + BigInt(fraction || '0')) * 3600n
  if (seconds % scale !== 0n || seconds > BigInt(90 * 86400) * scale) return undefined
  return Number(seconds / scale)
}

function poolWriteErrorCode(error: ApiError): string | undefined {
  if (typeof error.details !== 'object' || error.details === null || !('error' in error.details)) return undefined
  const detail = error.details.error
  return typeof detail === 'object' && detail !== null && 'code' in detail && typeof detail.code === 'string' ? detail.code : undefined
}

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
  const [errors, setErrors] = useState<PoolFieldErrors>({})
  const [serverError, setServerError] = useState<string>()
  const clearErrors = () => { setErrors({}); setServerError(undefined); action.reset() }
  const change = <T,>(setter: Dispatch<SetStateAction<T>>) => (value: T) => { setter(value); clearErrors() }

  const save = async () => {
    const next: PoolFieldErrors = {}
    const textError = (field: 'name' | 'description', value: string, max: number, required: boolean) => {
      const issue = field === 'description' ? merchantDescriptionError(value, max, required) : merchantTextError(value, max, required)
      if (issue) next[field] = t(`merchantPool.validation.${issue}`, { max })
    }
    textError('name', name, 128, true)
    textError('description', description, 2048, false)
    const groupId = positiveInteger(group)
    if (!groupId || (!pool && !(groups.data ?? []).some((item) => item.id === groupId))) next.group = t('merchantPool.validation.groupRequired')
    const minRatio = parsePoolRatio(minimum.trim()), maxRatio = parsePoolRatio(maximum.trim())
    for (const [field, value] of [['minimum', minRatio], ['maximum', maxRatio]] as const) {
      if (value === undefined || value <= 0) next[field] = t('merchantPool.validation.ratioPrecision')
      else if (value > 100_000_000) next[field] = t('merchantPool.validation.ratioRange', { min: '0.000001', max: '100' })
    }
    if (minRatio !== undefined && maxRatio !== undefined && !next.minimum && !next.maximum && minRatio > maxRatio) next.maximum = t('merchantPool.validation.ratioOrder')
    const commissionMicros = inheritGlobal ? undefined : parsePoolRatio(commission.trim())
    const commissionBps = inheritGlobal ? pool?.commission_bps ?? 0 : commissionMicros === undefined ? undefined : commissionMicros / 10_000
    if (commissionBps === undefined || !Number.isSafeInteger(commissionBps) || commissionBps < 0 || commissionBps > 10_000) next.commission = t('merchantPool.validation.commission')
    const originalDelay = String((pool?.settlement_delay_secs ?? 86400) / 3600)
    const settlementSeconds = settlementDelay === originalDelay ? pool?.settlement_delay_secs ?? 86400 : delaySeconds(settlementDelay.trim())
    if (settlementSeconds === undefined) next.settlementDelay = t('merchantPool.validation.settlementDelay')
    setErrors(next)
    setServerError(undefined)
    action.reset()
    if (Object.keys(next).length > 0 || groupId === undefined || minRatio === undefined || maxRatio === undefined || commissionBps === undefined || settlementSeconds === undefined) return
    const body: MerchantPoolWrite = { name: name.trim(), description: description.trim(), group_id: groupId, enabled, commission_bps: commissionBps, commission_inherit_global: inheritGlobal, min_ratio_micros: minRatio, max_ratio_micros: maxRatio, settlement_delay_secs: settlementSeconds }
    try {
      await action.mutateAsync({ url: pool ? `/api/admin/merchant-pools/${pool.id}` : '/api/admin/merchant-pools', method: pool ? 'put' : 'post', body })
      onClose()
    } catch (error) {
      const code = error instanceof ApiError ? poolWriteErrorCode(error) : undefined
      if (code === 'merchant_pool_conflict') {
        if (pool) setServerError(t('merchantPool.validation.poolConflict'))
        else setErrors({ group: t('merchantPool.validation.groupConflict') })
      } else if (code === 'merchant_pool_forbidden') setServerError(t('merchantPool.validation.poolForbidden'))
      else if (code === 'merchant_pool_not_found') setServerError(t('merchantPool.validation.poolNotFound'))
      else if (code === 'merchant_pool_invalid_request') setServerError(t('merchantPool.validation.poolInvalid'))
      else setServerError(t('merchantPool.saveError'))
    }
  }

  return <Drawer isOpen placement="right" size="xl" scrollBehavior="inside" onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader>{t(pool ? 'merchantPool.editPool' : 'merchantPool.createPool')}</DrawerHeader><DrawerBody className="gap-4 pb-5">
    <PoolField label={t('merchantPool.name')}><Input size="sm" aria-label={t('merchantPool.name')} value={name} onValueChange={change(setName)} isRequired isInvalid={Boolean(errors.name)} errorMessage={errors.name} isDisabled={action.isPending} /></PoolField>
    <PoolField label={t('merchantPool.description')} hint={t('merchantPool.validation.multilineHint')}><Textarea size="sm" aria-label={t('merchantPool.description')} value={description} onValueChange={change(setDescription)} isInvalid={Boolean(errors.description)} errorMessage={errors.description} isDisabled={action.isPending} /></PoolField>
    <PoolField label={t('merchantPool.group')} hint={t('merchantPool.poolGroupHint')}><PoolSelect label={t('merchantPool.group')} value={group} emptyLabel={t('merchantPool.validation.selectGroup')} disabled={Boolean(pool) || groups.isFetching || action.isPending} onChange={change(setGroup)} isInvalid={Boolean(errors.group)} errorMessage={errors.group} options={(groups.data ?? []).map((item) => ({ value: String(item.id), label: item.name }))} /></PoolField>
    {groups.isError ? <PoolError onRetry={() => void groups.refetch()} /> : null}
    <Checkbox size="sm" isSelected={inheritGlobal} onValueChange={change(setInheritGlobal)} isDisabled={action.isPending}>{t('merchantPool.inheritGlobalCommission')}</Checkbox>
    <PoolField label={t('merchantPool.commissionRate')}><Input size="sm" aria-label={t('merchantPool.commissionRate')} inputMode="decimal" isDisabled={inheritGlobal || action.isPending} value={commission} onValueChange={change(setCommission)} isInvalid={Boolean(errors.commission)} errorMessage={errors.commission} endContent="%" /></PoolField>
    <div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.minimumRatio')}><Input size="sm" aria-label={t('merchantPool.minimumRatio')} inputMode="decimal" value={minimum} onValueChange={change(setMinimum)} isInvalid={Boolean(errors.minimum)} errorMessage={errors.minimum} isDisabled={action.isPending} /></PoolField><PoolField label={t('merchantPool.maximumRatio')}><Input size="sm" aria-label={t('merchantPool.maximumRatio')} inputMode="decimal" value={maximum} onValueChange={change(setMaximum)} isInvalid={Boolean(errors.maximum)} errorMessage={errors.maximum} isDisabled={action.isPending} /></PoolField></div>
    <PoolField label={t('merchantPool.settlementDelay')}><Input size="sm" aria-label={t('merchantPool.settlementDelay')} type="number" min="0" max="2160" step="any" value={settlementDelay} onValueChange={change(setSettlementDelay)} isInvalid={Boolean(errors.settlementDelay)} errorMessage={errors.settlementDelay} isDisabled={action.isPending} /></PoolField>
    <Checkbox size="sm" isSelected={enabled} onValueChange={change(setEnabled)} isDisabled={action.isPending}>{t('merchantPool.enabled')}</Checkbox>
    {serverError ? <PoolError message={serverError} /> : null}
    <div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button><Button size="sm" color="primary" isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.save')}</Button></div>
  </DrawerBody></DrawerContent></Drawer>
}
