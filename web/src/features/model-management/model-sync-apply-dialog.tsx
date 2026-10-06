import { LoaderCircle } from 'lucide-react'
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

type ModelSyncApplyDialogProps = {
  count: number
  onApply: () => void
  onOpenChange: (open: boolean) => void
  open: boolean
  pending: boolean
}

export function ModelSyncApplyDialog(props: ModelSyncApplyDialogProps) {
  const { t } = useTranslation()
  return (
    <AlertDialog open={props.open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('modelManagement.sync.apply.title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('modelManagement.sync.apply.description', { count: props.count })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="rounded-lg bg-surface-2/55 px-3 py-2 text-xs leading-5 text-muted-foreground">
          {t('modelManagement.sync.apply.boundary')}
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={props.pending}>{t('modelManagement.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={props.pending} onClick={(event) => { event.preventDefault(); props.onApply() }}>
            {props.pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t('modelManagement.sync.actions.apply')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
