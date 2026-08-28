import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { OrganizationServiceAccountIssuedResponse } from '@/lib/api/generated/types.gen'
import { useCreateOrganizationServiceAccount, organizationServiceAccountErrorCode } from './organization-service-account-api'

export function OrganizationServiceAccountEditorSheet({
  organizationId,
  open,
  onOpenChange,
  onIssued,
}: {
  organizationId: number
  open: boolean
  onOpenChange: (open: boolean) => void
  onIssued: (issued: OrganizationServiceAccountIssuedResponse) => void
}) {
  const { t } = useTranslation()
  const mutation = useCreateOrganizationServiceAccount(organizationId)
  const resetMutation = mutation.reset
  const [name, setName] = useState('')
  const [touched, setTouched] = useState(false)

  useEffect(() => {
    if (!open) return
    setName('')
    setTouched(false)
    resetMutation()
  }, [open, resetMutation])

  const invalid = touched && name.trim().length === 0
  const submit = async () => {
    setTouched(true)
    const normalizedName = name.trim()
    if (!normalizedName) return
    try {
      const issued = await mutation.mutateAsync({ name: normalizedName })
      mutation.reset()
      onOpenChange(false)
      onIssued(issued)
    } catch {
      // 保留输入，让管理员可在冲突或服务暂不可用时重试。
    }
  }
  const errorKey = mutation.error
    ? organizationServiceAccountErrorCode(mutation.error) === 'organization_entitlement_restricted'
      ? 'organizationServiceAccounts.errors.entitlement'
      : 'organizationServiceAccounts.errors.create'
    : undefined

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full gap-0 sm:max-w-lg">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('organizationServiceAccounts.editor.title')}</SheetTitle>
          <SheetDescription>{t('organizationServiceAccounts.editor.description')}</SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex-1 overflow-y-auto p-4">
            <div className="grid gap-1.5">
              <label htmlFor="organization-service-account-name" className="text-xs font-medium">
                {t('organizationServiceAccounts.editor.name')}
              </label>
              <Input
                id="organization-service-account-name"
                value={name}
                maxLength={120}
                autoFocus
                aria-invalid={invalid}
                placeholder={t('organizationServiceAccounts.editor.namePlaceholder')}
      onChange={(event) => { setName(event.target.value); resetMutation() }}
                onBlur={() => setTouched(true)}
              />
              {invalid ? <p className="text-xs text-destructive">{t('organizationServiceAccounts.editor.nameRequired')}</p> : null}
              <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('organizationServiceAccounts.editor.nameHint')}</p>
            </div>
            {errorKey ? <p role="alert" className="mt-4 rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t(errorKey)}</p> : null}
          </div>
          <SheetFooter className="border-t border-[var(--hairline)]">
            <Button type="button" variant="outline" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationServiceAccounts.actions.cancel')}</Button>
            <Button type="button" disabled={mutation.isPending} onClick={() => void submit()}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
              {t('organizationServiceAccounts.actions.create')}
            </Button>
          </SheetFooter>
        </div>
      </SheetContent>
    </Sheet>
  )
}
