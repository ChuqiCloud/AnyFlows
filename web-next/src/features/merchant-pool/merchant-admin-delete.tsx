import { Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { ApiError } from '@/lib/api'
import { useMerchantAction } from './merchant-pool-api'
import type { MerchantPool, MerchantProfile } from './merchant-pool-model'
import { PoolButton as Button, PoolError } from './merchant-pool-ui'

export type MerchantDeleteTarget = { kind: 'pool'; item: MerchantPool } | { kind: 'merchant'; item: MerchantProfile }

function deleteErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null || !('error' in error.details)) return undefined
  const detail = error.details.error
  return typeof detail === 'object' && detail !== null && 'code' in detail && typeof detail.code === 'string' ? detail.code : undefined
}

export function MerchantAdminDelete({ target, onClose }: { target: MerchantDeleteTarget; onClose: () => void }) {
  const { t } = useTranslation()
  const action = useMerchantAction()
  const remove = async () => {
    if (action.isPending) return
    try {
      await action.mutateAsync({ url: target.kind === 'pool' ? `/api/admin/merchant-pools/${target.item.id}` : `/api/admin/merchants/${target.item.id}`, method: 'delete' })
      onClose()
    } catch { /* Keep the selected resource and confirmation open for retry. */ }
  }
  const code = deleteErrorCode(action.error)
  const errorKey = code === 'merchant_pool_in_use' ? 'inUse' : action.error instanceof ApiError && action.error.status === 403 ? 'forbidden' : action.error instanceof ApiError && action.error.status === 404 ? 'notFound' : action.error instanceof ApiError && action.error.status === 409 ? 'conflict' : 'error'
  return <Modal isOpen isDismissable={!action.isPending} onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t(`merchantPool.deletion.${target.kind}Title`)}</ModalHeader><ModalBody className="gap-3"><p className="break-words text-sm font-medium">{target.item.name}</p><p className="text-xs leading-6 text-muted-foreground">{t(`merchantPool.deletion.${target.kind}Hint`)}</p>{action.isError ? <PoolError message={t(`merchantPool.deletion.${errorKey}`)} /> : null}</ModalBody><ModalFooter><Button size="sm" variant="light" isDisabled={action.isPending} onPress={onClose}>{t('merchantPool.cancel')}</Button><Button size="sm" color="danger" isLoading={action.isPending} onPress={() => void remove()}>{t('merchantPool.delete')}</Button></ModalFooter></ModalContent></Modal>
}
