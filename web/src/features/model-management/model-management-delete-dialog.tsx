import type { MouseEvent } from 'react'
import { LoaderCircle, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { buttonVariants } from '@/components/ui/button'
import type { AdminModel } from '@/lib/api/generated/types.gen'
import { useDeleteAdminModel } from './model-management-api'

type ModelManagementDeleteDialogProps = {
  model?: AdminModel
  onClose: () => void
}

export function ModelManagementDeleteDialog({ model, onClose }: ModelManagementDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminModel()

  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!model) return
    try {
      await mutation.mutateAsync(model.id)
      onClose()
    } catch {
      // 删除失败时保持确认框打开，避免管理员误判元数据状态。
    }
  }

  return (
    <AlertDialog open={model !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('modelManagement.delete.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('modelManagement.delete.description', { model: model?.model })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('modelManagement.delete.failed')}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('modelManagement.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={mutation.isPending} onClick={confirm}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}
            {t('modelManagement.actions.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
