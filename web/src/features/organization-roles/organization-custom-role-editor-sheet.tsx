import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Textarea } from '@/components/ui/textarea'
import type { OrganizationCustomRoleResponse } from '@/lib/api/generated/types.gen'
import {
  organizationCustomRoleErrorCode,
  useCreateOrganizationCustomRole,
  useUpdateOrganizationCustomRole,
} from './organization-custom-role-api'
import { CUSTOM_ROLE_PERMISSION_CODES, selectedCustomRolePermissions } from './organization-custom-role-permissions'

export function OrganizationCustomRoleEditorSheet({
  organizationId,
  organizationName,
  role,
  open,
  onOpenChange,
}: {
  organizationId: number
  organizationName: string
  role?: OrganizationCustomRoleResponse
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const createMutation = useCreateOrganizationCustomRole(organizationId)
  const updateMutation = useUpdateOrganizationCustomRole(organizationId)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [permissions, setPermissions] = useState<string[]>([])
  const mutation = role ? updateMutation : createMutation
  const resetCreateMutation = createMutation.reset
  const resetUpdateMutation = updateMutation.reset
  const validName = name.trim() === name && name.length > 0 && new TextEncoder().encode(name).length <= 160
  const selectedPermissions = selectedCustomRolePermissions(permissions)
  const valid = validName && selectedPermissions.length > 0

  useEffect(() => {
    if (!open) return
    setName(role?.name ?? '')
    setDescription(role?.description ?? '')
    setPermissions(role ? selectedCustomRolePermissions(role.permissions) : [])
    resetCreateMutation()
    resetUpdateMutation()
  }, [open, resetCreateMutation, resetUpdateMutation, role])

  const togglePermission = (permission: string, checked: boolean) => {
    setPermissions((current) => {
      const next = checked ? [...current, permission] : current.filter((item) => item !== permission)
      return selectedCustomRolePermissions(next)
    })
    mutation.reset()
  }

  const save = async () => {
    if (!valid) return
    const body = {
      name,
      description: description.trim() ? description : null,
      permissions: selectedPermissions,
    }
    try {
      if (role) {
        await updateMutation.mutateAsync({
          roleKey: role.role_key,
          body: { ...body, expected_version: role.version },
        })
      } else {
        await createMutation.mutateAsync(body)
      }
      onOpenChange(false)
    } catch {
      // CAS 冲突或未知结果时保留草稿，刷新目录后可按最新角色事实再次提交。
    }
  }

  return (
    <Sheet open={open} onOpenChange={(nextOpen) => { if (!nextOpen && !mutation.isPending) onOpenChange(false) }}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-xl">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(role ? 'organizationConsole.customRoles.editor.editTitle' : 'organizationConsole.customRoles.editor.createTitle')}</SheetTitle>
          <SheetDescription>{t('organizationConsole.customRoles.editor.description', { organization: organizationName })}</SheetDescription>
        </SheetHeader>
        <div className="grid gap-4 overflow-y-auto p-4">
          <div>
            <label htmlFor="organization-custom-role-name" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.customRoles.fields.name')}</label>
            <Input id="organization-custom-role-name" value={name} maxLength={160} disabled={mutation.isPending} aria-invalid={!validName && name.length > 0} onChange={(event) => { setName(event.target.value); mutation.reset() }} />
            {!validName && name.length > 0 ? <p className="mt-1 text-xs text-destructive">{t('organizationConsole.customRoles.validation.name')}</p> : null}
          </div>
          <div>
            <label htmlFor="organization-custom-role-description" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.customRoles.fields.description')}</label>
            <Textarea id="organization-custom-role-description" rows={4} maxLength={500} value={description} disabled={mutation.isPending} onChange={(event) => { setDescription(event.target.value); mutation.reset() }} />
          </div>
          <fieldset disabled={mutation.isPending}>
            <legend className="text-xs font-medium">{t('organizationConsole.customRoles.fields.permissions')}</legend>
            <p id="organization-custom-role-permissions-hint" className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.customRoles.editor.permissionsHint')}</p>
            <div className="mt-3 grid gap-2 sm:grid-cols-2">
              {CUSTOM_ROLE_PERMISSION_CODES.map((permission) => {
                const id = `organization-custom-role-permission-${permission.replaceAll('.', '-')}`
                const checked = selectedPermissions.includes(permission)
                return (
                  <label key={permission} htmlFor={id} className="flex min-w-0 items-start gap-2 rounded-lg border border-[var(--hairline)] px-3 py-2.5 text-xs transition-colors hover:bg-surface-2/55">
                    <Checkbox id={id} checked={checked} aria-describedby="organization-custom-role-permissions-hint" onCheckedChange={(value) => togglePermission(permission, value === true)} />
                    <span className="min-w-0 leading-5">{t(`organizationConsole.permissions.${permission}`)}</span>
                  </label>
                )
              })}
            </div>
            {selectedPermissions.length === 0 ? <p className="mt-2 text-xs text-destructive">{t('organizationConsole.customRoles.validation.permissions')}</p> : null}
          </fieldset>
          {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t(`organizationConsole.customRoles.errors.${customRoleErrorKey(organizationCustomRoleErrorCode(mutation.error))}`)}</p> : null}
        </div>
        <SheetFooter className="mt-auto border-t border-[var(--hairline)]">
          <Button type="button" variant="outline" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
          <Button type="button" disabled={!valid || mutation.isPending} onClick={() => void save()}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
            {t('organizationConsole.actions.save')}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

function customRoleErrorKey(code: string | undefined) {
  if (code === 'organization_lifecycle_conflict') return 'conflict'
  if (code === 'organization_lifecycle_transition_invalid') return 'inactive'
  if (code === 'organization_entitlement_restricted') return 'entitlement'
  if (code === 'organization_not_found') return 'notFound'
  return 'mutation'
}
