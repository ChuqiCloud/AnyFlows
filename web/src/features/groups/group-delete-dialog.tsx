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
import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { groupWriteErrorCode, useDeleteAdminGroup } from './group-api'

type GroupDeleteDialogProps = {
  group?: AdminGroup
  onClose: () => void
}

export function GroupDeleteDialog({ group, onClose }: GroupDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminGroup()

  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!group) return
    try {
      await mutation.mutateAsync(group.id)
      onClose()
    } catch {
      // 引用冲突或运行时刷新失败时保持确认框打开，避免误报删除成功。
    }
  }

  const errorCode = mutation.error ? groupWriteErrorCode(mutation.error) ?? 'unknown' : undefined
  return (
    <AlertDialog open={group !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('groups.delete.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('groups.delete.description', { name: group?.display_name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {errorCode ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t(`groups.errors.${errorCode}`, { defaultValue: t('groups.errors.unknown') })}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('groups.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={mutation.isPending} onClick={confirm}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}
            {t('groups.actions.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
