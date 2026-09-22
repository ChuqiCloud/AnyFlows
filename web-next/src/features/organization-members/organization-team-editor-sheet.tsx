import { Button, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Input, Textarea } from '@heroui/react'
import { LoaderCircle, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-lg' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t(team ? 'organizationConsole.teams.editor.editTitle' : 'organizationConsole.teams.editor.createTitle')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationConsole.teams.editor.description', { organization: organizationName })}</p>
            </DrawerHeader>
            <DrawerBody className="gap-4 p-4">
              <div>
                <label htmlFor="organization-team-name" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.teams.fields.name')}</label>
                <Input id="organization-team-name" value={name} maxLength={160} isDisabled={mutation.isPending} isInvalid={!valid && name.length > 0} size="sm" onChange={(event) => { setName(event.target.value); mutation.reset() }} />
                {!valid && name.length > 0 ? <p className="mt-1 text-xs text-destructive">{t('organizationConsole.teams.validation.name')}</p> : null}
              </div>
              <div>
                <label htmlFor="organization-team-description" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.teams.fields.description')}</label>
                <Textarea id="organization-team-description" minRows={5} maxLength={500} value={description} isDisabled={mutation.isPending} onChange={(event) => { setDescription(event.target.value); mutation.reset() }} />
              </div>
              {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(mutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
            </DrawerBody>
            <DrawerFooter className="mt-auto border-t border-[var(--hairline)]">
              <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
              <Button type="button" color="primary" isDisabled={!valid || mutation.isPending} onClick={() => void save()}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Save className="size-4" aria-hidden="true" />}
                {t('organizationConsole.actions.save')}
              </Button>
            </DrawerFooter>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
