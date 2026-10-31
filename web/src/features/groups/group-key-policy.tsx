import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
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
import {
  useAdminGroupKeyPolicy,
  useUpdateAdminGroupKeyPolicy,
  type GroupKeyPolicy,
  type GroupKeySelectionMode,
} from './group-key-policy-api'
import { GroupKeyPolicyGroups } from './group-key-policy-groups'
import { GroupKeyPolicyUsers } from './group-key-policy-users'

export function GroupKeyPolicyEditor({ groupId }: { groupId: number }) {
  const { t } = useTranslation()
  const policyQuery = useAdminGroupKeyPolicy(groupId)
  if (policyQuery.isPending) return <p role="status" className="p-4 text-sm text-muted-foreground">{t('groups.keyPolicy.loading')}</p>
  if (policyQuery.isError) {
    return (
      <div role="alert" className="m-4 grid gap-3 rounded-lg border border-destructive/25 p-4 text-sm text-destructive">
        <p>{t('groups.keyPolicy.loadError')}</p>
        <Button type="button" size="sm" variant="secondary" onClick={() => void policyQuery.refetch()}>{t('groups.actions.retry')}</Button>
      </div>
    )
  }
  return <GroupKeyPolicyForm key={groupId} policy={policyQuery.data} />
}

function GroupKeyPolicyForm({ policy }: { policy: GroupKeyPolicy }) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminGroupKeyPolicy(policy.group_id)
  const [mode, setMode] = useState<GroupKeySelectionMode>(policy.selection_mode)
  const [selectedUserIds, setSelectedUserIds] = useState(policy.allowed_user_ids)
  const [selectedGroupIds, setSelectedGroupIds] = useState(policy.allowed_group_ids)
  const [disableKeys, setDisableKeys] = useState(false)
  const [confirmRevocation, setConfirmRevocation] = useState(false)
  useEffect(() => {
    setMode(policy.selection_mode)
    setSelectedUserIds(policy.allowed_user_ids)
    setSelectedGroupIds(policy.allowed_group_ids)
    setDisableKeys(false)
  }, [policy])
  const modes = ['default_only', 'all_users', 'selected_users', 'selected_groups'] as const

  return (
    <>
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => {
      event.preventDefault()
      if (mutation.isPending) return
      if (disableKeys) {
        setConfirmRevocation(true)
        return
      }
      mutation.mutate({ selection_mode: mode, allowed_user_ids: mode === 'selected_users' ? selectedUserIds : [], allowed_group_ids: mode === 'selected_groups' ? selectedGroupIds : [], disable_keys_losing_access: false })
    }}>
      <div className="flex-1 space-y-4 overflow-y-auto px-4 py-5">
        <div>
          <h3 className="text-sm font-semibold">{t('groups.keyPolicy.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.description')}</p>
        </div>
        <div className="grid gap-1.5">
          <label className="text-xs font-medium" htmlFor="group-key-policy-mode">{t('groups.keyPolicy.modeLabel')}</label>
          <Select id="group-key-policy-mode" value={mode} disabled={mutation.isPending} onChange={(event) => { setMode(event.target.value as GroupKeySelectionMode); mutation.reset() }}>
            {modes.map((item) => <option key={item} value={item}>{t(`groups.keyPolicy.modes.${item}`)}</option>)}
          </Select>
        </div>
        <p className="rounded-lg bg-surface-2 px-3 py-2 text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.defaultAccess')}</p>
        {mode === 'selected_users' ? <GroupKeyPolicyUsers selectedIds={selectedUserIds} disabled={mutation.isPending} onChange={(ids) => { setSelectedUserIds(ids); mutation.reset() }} /> : null}
        {mode === 'selected_groups' ? <GroupKeyPolicyGroups selectedIds={selectedGroupIds} disabled={mutation.isPending} onChange={(ids) => { setSelectedGroupIds(ids); mutation.reset() }} /> : null}
        <label className="flex items-start gap-2 rounded-lg border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs leading-5">
          <input type="checkbox" className="mt-0.5 size-4 accent-destructive" checked={disableKeys} disabled={mutation.isPending} onChange={(event) => { setDisableKeys(event.target.checked); mutation.reset() }} />
          <span><strong>{t('groups.keyPolicy.disableExistingTitle')}</strong><br />{t('groups.keyPolicy.disableExistingDescription')}</span>
        </label>
        <p className="text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.revokeHint')}</p>
        {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t('groups.keyPolicy.saveError')}</p> : null}
        {mutation.isSuccess ? <p role="status" className="text-xs text-success">{t('groups.keyPolicy.saved')} {t('groups.keyPolicy.disabledCount', { count: mutation.data.disabled_key_count })} {t('groups.keyPolicy.disabledUserCount', { count: mutation.data.disabled_user_count })}</p> : null}
      </div>
      <div className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] p-4">
        <p className="text-xs text-muted-foreground">{t('groups.keyPolicy.independentSave')}</p>
        <Button type="submit" disabled={mutation.isPending}>{t(mutation.isPending ? 'groups.keyPolicy.saving' : 'groups.keyPolicy.save')}</Button>
      </div>
    </form>
    <AlertDialog open={confirmRevocation} onOpenChange={(open) => !mutation.isPending && setConfirmRevocation(open)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('groups.keyPolicy.confirmDisableTitle')}</AlertDialogTitle>
          <AlertDialogDescription>{t('groups.keyPolicy.confirmDisableDescription')}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>{t('groups.actions.cancel')}</AlertDialogCancel>
          <AlertDialogAction disabled={mutation.isPending} onClick={(event) => {
            event.preventDefault()
            setConfirmRevocation(false)
          mutation.mutate({ selection_mode: mode, allowed_user_ids: mode === 'selected_users' ? selectedUserIds : [], allowed_group_ids: mode === 'selected_groups' ? selectedGroupIds : [], disable_keys_losing_access: true })
          }}>{t('groups.keyPolicy.confirmDisableAction')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
    </>
  )
}
