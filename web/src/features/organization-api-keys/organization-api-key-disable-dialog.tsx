import { Power } from 'lucide-react'
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
import type { OrganizationTokenResponse } from '@/lib/api/generated/types.gen'
import { useDisableOrganizationApiKey } from './organization-api-key-api'

type OrganizationApiKeyDisableDialogProps = {
  organizationId: number
  organizationName: string
  directory: boolean
  token?: OrganizationTokenResponse
  onClose: () => void
}

export function OrganizationApiKeyDisableDialog(props: OrganizationApiKeyDisableDialogProps) {
  const { t } = useTranslation()
  const mutation = useDisableOrganizationApiKey(props.organizationId, props.directory)
  const disable = async () => {
    if (!props.token) return
    try {
      await mutation.mutateAsync(props.token.id)
      props.onClose()
    } catch {
      // 对话框保持打开，让用户明确看到停用失败。
    }
  }

  return (
    <AlertDialog open={props.token !== undefined} onOpenChange={(open) => !open && props.onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-warning/10 text-warning"><Power className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('organizationApiKeys.disable.title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t(props.directory ? 'organizationApiKeys.disable.directoryDescription' : 'organizationApiKeys.disable.selfDescription', {
              name: props.token?.name,
              organization: props.organizationName,
              owner: props.token?.owner_user_id,
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('organizationApiKeys.disable.failed')}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('apiKeys.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={mutation.isPending} onClick={() => void disable()}>{t('organizationApiKeys.actions.disable')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
