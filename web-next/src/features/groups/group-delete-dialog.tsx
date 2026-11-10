import { Button, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useState } from 'react'

import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { groupWriteErrorCode, useAdminGroupCatalog, useDeleteAdminGroup, useMerchantPoolGroupBindings } from './group-api'

type GroupDeleteDialogProps = {
  group?: AdminGroup
  onClose: () => void
}

export function GroupDeleteDialog({ group, onClose }: GroupDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminGroup()
  const groups = useAdminGroupCatalog()
  const poolGroups = useMerchantPoolGroupBindings(group !== undefined)
  const [replacement, setReplacement] = useState('')
  const targets = (groups.data ?? []).filter((item) => item.id !== group?.id && !(poolGroups.data ?? []).includes(item.id))
  const target = targets.find((item) => String(item.id) === replacement)
  const targetsUnavailable = groups.isPending || poolGroups.isPending || groups.isError || poolGroups.isError

  const confirm = async () => {
    if (!group || mutation.isPending || (replacement !== '' && (!target || targetsUnavailable))) return
    try {
      await mutation.mutateAsync({ id: group.id, replacementGroupId: target?.id })
      onClose()
    } catch {
      // 引用冲突或运行时刷新失败时保持确认框打开，避免误报删除成功。
    }
  }

  const errorCode = mutation.error ? groupWriteErrorCode(mutation.error) ?? 'unknown' : undefined
  return (
    <Modal
      backdrop="blur"
      hideCloseButton
      isDismissable={false}
      isOpen={group !== undefined}
      onOpenChange={(open) => !open && !mutation.isPending && onClose()}
    >
      <ModalContent>
        {() => (
          <>
            <ModalHeader className="grid gap-1.5">
              <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
              <h2 className="text-base font-semibold">{t('groups.delete.title')}</h2>
              <p className="text-sm leading-5 font-normal text-muted-foreground">{t('groups.delete.description', { name: group?.display_name })}</p>
            </ModalHeader>
            <ModalBody className="gap-3">
              <label className="grid min-w-0 gap-1.5 text-xs"><span className="font-medium">{t('groups.delete.replacement')}</span><select aria-label={t('groups.delete.replacement')} className="min-h-9 w-full min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-2.5 text-xs" value={replacement} disabled={targetsUnavailable || mutation.isPending} onChange={(event) => { setReplacement(event.target.value); mutation.reset() }}><option value="">{t('groups.delete.noReplacement')}</option>{targets.map((item) => <option key={item.id} value={item.id}>{item.display_name}</option>)}</select></label>
              <p className="text-xs leading-5 text-muted-foreground">{t(target ? 'groups.delete.transferHint' : 'groups.delete.replacementHint', { name: target?.display_name })}</p>
              {groups.isError || poolGroups.isError ? <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive"><span>{t('groups.delete.targetsError')}</span><Button size="sm" variant="light" onPress={() => { void groups.refetch(); void poolGroups.refetch() }}>{t('groups.actions.retry')}</Button></div> : null}
              {errorCode ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs font-normal text-destructive">{t(`groups.errors.${errorCode}`, { defaultValue: t('groups.errors.unknown') })}</p> : null}
            </ModalBody>
            <ModalFooter>
              <Button variant="light" isDisabled={mutation.isPending} onPress={onClose}>{t('groups.actions.cancel')}</Button>
              <Button color="danger" variant="flat" isDisabled={mutation.isPending || (replacement !== '' && (!target || targetsUnavailable))} onPress={() => void confirm()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}
                {t('groups.actions.delete')}
              </Button>
            </ModalFooter>
          </>
        )}
      </ModalContent>
    </Modal>
  )
}
