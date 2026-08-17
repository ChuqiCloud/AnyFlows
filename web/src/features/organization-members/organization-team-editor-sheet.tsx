import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Textarea } from '@/components/ui/textarea'
import type { OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useChangeOrganizationTeam, useCreateOrganizationTeam } from './organization-member-api'

export function OrganizationTeamEditorSheet({
  organizationId,
  organizationName,
  team,
  open,
  onOpenChange,
}: {
  organizationId: number
  organizationName: string
  team?: OrganizationTeamResponse
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const createMutation = useCreateOrganizationTeam(organizationId)
  const changeMutation = useChangeOrganizationTeam(organizationId)
  const resetCreateMutation = createMutation.reset
  const resetChangeMutation = changeMutation.reset
  const mutation = team ? changeMutation : createMutation
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')

  useEffect(() => {
    if (!open) return
    setName(team?.name ?? '')
    setDescription(team?.description ?? '')
    resetCreateMutation()
    resetChangeMutation()
  }, [open, resetChangeMutation, resetCreateMutation, team])

  const valid = name.trim() === name && name.length > 0 && new TextEncoder().encode(name).length <= 160
  const save = async () => {
    if (!valid) return
    const body = { name, description: description.trim() ? description : null }
    try {
      if (team) await changeMutation.mutateAsync({ teamId: team.id, body: { ...body, expected_version: team.version } })
      else await createMutation.mutateAsync(body)
      onOpenChange(false)
    } catch {
      // 冲突或容量变化时保留表单，刷新后可继续核对。
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-lg">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t(team ? 'organizationConsole.teams.editor.editTitle' : 'organizationConsole.teams.editor.createTitle')}</SheetTitle>
          <SheetDescription>{t('organizationConsole.teams.editor.description', { organization: organizationName })}</SheetDescription>
        </SheetHeader>
        <div className="grid gap-4 overflow-y-auto p-4">
          <div>
            <label htmlFor="organization-team-name" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.teams.fields.name')}</label>
            <Input id="organization-team-name" value={name} maxLength={160} disabled={mutation.isPending} aria-invalid={!valid && name.length > 0} onChange={(event) => { setName(event.target.value); mutation.reset() }} />
            {!valid && name.length > 0 ? <p className="mt-1 text-xs text-destructive">{t('organizationConsole.teams.validation.name')}</p> : null}
          </div>
          <div>
            <label htmlFor="organization-team-description" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.teams.fields.description')}</label>
            <Textarea id="organization-team-description" rows={5} maxLength={500} value={description} disabled={mutation.isPending} onChange={(event) => { setDescription(event.target.value); mutation.reset() }} />
          </div>
          {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
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
