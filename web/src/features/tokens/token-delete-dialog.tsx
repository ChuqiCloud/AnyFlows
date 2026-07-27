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
import type { AdminToken } from '@/lib/api/generated/types.gen'
import { useDeleteAdminToken } from './token-api'

type TokenDeleteDialogProps = {
  token?: AdminToken
  onClose: () => void
  onDeleted: () => void
}

export function TokenDeleteDialog({ token, onClose, onDeleted }: TokenDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminToken()

  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!token) return
    try {
      await mutation.mutateAsync(token.id)
      onDeleted()
    } catch {
      // 失败时保持确认框打开，避免管理员误以为令牌已经失效。
    }
  }

  return (
    <AlertDialog open={token !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('tokens.delete.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('tokens.delete.description', { name: token?.name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('tokens.delete.failed')}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('tokens.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={mutation.isPending} onClick={confirm}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}{t('tokens.actions.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
