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
import type { AdminRoute } from '@/lib/api/generated/types.gen'
import { routeWriteErrorCode, useDeleteAdminRoute } from './route-api'

type RouteDeleteDialogProps = {
  route?: AdminRoute
  onClose: () => void
}

export function RouteDeleteDialog({ route, onClose }: RouteDeleteDialogProps) {
  const { t } = useTranslation()
  const mutation = useDeleteAdminRoute()

  const confirm = async (event: MouseEvent<HTMLButtonElement>) => {
    event.preventDefault()
    if (!route) return
    try {
      await mutation.mutateAsync(route.id)
      onClose()
    } catch {
      // 删除冲突时保留确认框，让管理员看到稳定错误并决定下一步。
    }
  }

  const errorCode = mutation.error ? routeWriteErrorCode(mutation.error) : undefined
  return (
    <AlertDialog open={route !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive"><Trash2 className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('routes.delete.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('routes.delete.description', { name: route?.name ?? '' })}</AlertDialogDescription>
        </AlertDialogHeader>
        {errorCode ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t(`routes.errors.${errorCode}`, { defaultValue: t('routes.errors.unknown') })}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('routes.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction className={buttonVariants({ variant: 'destructive' })} disabled={mutation.isPending} onClick={confirm}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Trash2 aria-hidden="true" />}
            {t('routes.actions.delete')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
