import { Ban, LoaderCircle } from 'lucide-react'
import { useEffect } from 'react'
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
import type { OrganizationCustomRoleResponse } from '@/lib/api/generated/types.gen'
import { organizationCustomRoleErrorCode, useDisableOrganizationCustomRole } from './organization-custom-role-api'

export function OrganizationCustomRoleDisableDialog({
  organizationId,
  role,
  onClose,
}: {
  organizationId: number
  role?: OrganizationCustomRoleResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const mutation = useDisableOrganizationCustomRole(organizationId)
  const resetMutation = mutation.reset

  useEffect(() => {
    if (role) resetMutation()
  }, [resetMutation, role])

  const disable = async () => {
    if (!role) return
    try {
      await mutation.mutateAsync({ roleKey: role.role_key, expectedVersion: role.version })
      onClose()
    } catch {
      // 保留确认目标与错误提示，避免把 CAS 冲突误显示为角色已经停用。
    }
  }

  return (
    <AlertDialog open={role !== undefined} onOpenChange={(open) => !open && !mutation.isPending && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-destructive/10 text-destructive">
            <Ban className="size-4" aria-hidden="true" />
          </div>
          <AlertDialogTitle>{t('organizationConsole.customRoles.disable.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('organizationConsole.customRoles.disable.description', { name: role?.name })}</AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t(`organizationConsole.customRoles.errors.${disableErrorKey(organizationCustomRoleErrorCode(mutation.error))}`)}</p> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('organizationConsole.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction
            className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            disabled={mutation.isPending}
            onClick={(event) => {
              event.preventDefault()
              void disable()
            }}
          >
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Ban aria-hidden="true" />}
            {t(mutation.isPending ? 'organizationConsole.customRoles.actions.disabling' : 'organizationConsole.customRoles.actions.disable')}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

function disableErrorKey(code: string | undefined) {
  if (code === 'organization_lifecycle_conflict') return 'conflict'
  if (code === 'organization_lifecycle_transition_invalid') return 'inactive'
  if (code === 'organization_entitlement_restricted') return 'entitlement'
  if (code === 'organization_not_found') return 'notFound'
  return 'mutation'
}
