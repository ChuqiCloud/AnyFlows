import { useState } from 'react'
import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction } from './merchant-pool-api'
import { PoolError, PoolField } from './merchant-pool-ui'

export function MerchantKeyRotation({ serviceId, onClose }: { serviceId: number; onClose: () => void }) {
  const { t } = useTranslation()
  const [key, setKey] = useState('')
  const action = useMerchantAction()
  const save = async () => { if (!key.trim()) return; try { await action.mutateAsync({ url: `/api/merchant/services/${serviceId}/key`, body: { api_key: key.trim() } }); setKey(''); action.reset(); onClose() } catch { /* The key stays only in this dialog's memory for retry. */ } }
  return <Modal isOpen onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><ModalContent><ModalHeader>{t('merchantPool.rotateKey')}</ModalHeader><ModalBody><p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.rotateKeyHint')}</p><PoolField label={t('merchantPool.upstreamKey')}><Input size="sm" aria-label={t('merchantPool.upstreamKey')} type="password" autoComplete="new-password" value={key} onValueChange={setKey} /></PoolField>{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}</ModalBody><ModalFooter><Button size="sm" variant="flat" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button><Button size="sm" color="primary" isDisabled={!key.trim()} isLoading={action.isPending} onPress={() => void save()}>{t('merchantPool.confirm')}</Button></ModalFooter></ModalContent></Modal>
}
