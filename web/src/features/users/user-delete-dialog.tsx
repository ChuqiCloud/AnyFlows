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
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { useDeleteAdminUser } from './user-api'

type UserDeleteDialogProps = {
  user?: AdminUser
  onClose: () => void
}

export function UserDeleteDialog({ user, onClose }: UserDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminUser()

  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!user) return
    try {
      await mutation.mutateAsync(user.id)
      onClose()
    } catch {
      // 失败时保持确认框打开，避免管理员误以为账户已经删除。
    }
  }

  return (
    <AlertDialog open={user !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('users.delete.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('users.delete.description', { username: user?.username })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('users.delete.failed')}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('users.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={mutation.isPending} onClick={confirm}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}
            {t('users.actions.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
