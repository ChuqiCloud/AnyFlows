import { useEffect, useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button, Select, SelectItem } from '@heroui/react'
import ConfirmModal from '@/shared/components/confirm-modal'
import {
  useAdminGroupKeyPolicy,
  useUpdateAdminGroupKeyPolicy,
  type GroupKeyPolicy,
  type GroupKeySelectionMode,
} from './group-key-policy-api'
import { GroupKeyPolicyUsers } from './group-key-policy-users'

export function GroupKeyPolicyEditor({ groupId, portalContainer }: { groupId: number; portalContainer?: HTMLElement | null }) {
  const { t } = useTranslation()
  const policyQuery = useAdminGroupKeyPolicy(groupId)
  if (policyQuery.isPending) return <p role="status" className="p-4 text-sm text-muted-foreground">{t('groups.keyPolicy.loading')}</p>
  if (policyQuery.isError) {
    return (
      <div role="alert" className="m-4 grid gap-3 rounded-lg border border-destructive/25 p-4 text-sm text-destructive">
        <p>{t('groups.keyPolicy.loadError')}</p>
        <Button type="button" size="sm" variant="flat" onClick={() => void policyQuery.refetch()}>{t('groups.actions.retry')}</Button>
      </div>
    )
  }
  return <GroupKeyPolicyForm key={groupId} policy={policyQuery.data} portalContainer={portalContainer} />
}

function GroupKeyPolicyForm({ policy, portalContainer }: { policy: GroupKeyPolicy; portalContainer?: HTMLElement | null }) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminGroupKeyPolicy(policy.group_id)
  const labelId = useId()
  const [mode, setMode] = useState<GroupKeySelectionMode>(policy.selection_mode)
  const [selectedIds, setSelectedIds] = useState(policy.allowed_user_ids)
  const [disableKeys, setDisableKeys] = useState(false)
  const [confirmRevocation, setConfirmRevocation] = useState(false)
  useEffect(() => {
    setMode(policy.selection_mode)
    setSelectedIds(policy.allowed_user_ids)
    setDisableKeys(false)
  }, [policy])
  const modes = ['default_only', 'all_users', 'selected_users'] as const

  return (
    <>
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => {
      event.preventDefault()
      if (mutation.isPending) return
      if (disableKeys) {
        setConfirmRevocation(true)
        return
      }
      mutation.mutate({ selection_mode: mode, allowed_user_ids: mode === 'selected_users' ? selectedIds : [], disable_keys_losing_access: false })
    }}>
      <div className="flex-1 space-y-4 overflow-y-auto px-4 py-5">
        <div>
          <h3 className="text-sm font-semibold">{t('groups.keyPolicy.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.description')}</p>
        </div>
        <div className="grid gap-1.5">
          <label id={labelId} className="text-xs font-medium" htmlFor="group-key-policy-mode">{t('groups.keyPolicy.modeLabel')}</label>
          <Select
            aria-label={t('groups.keyPolicy.modeLabel')}
            aria-labelledby={labelId}
            popoverProps={portalContainer ? { portalContainer } : undefined}
            id="group-key-policy-mode"
            isDisabled={mutation.isPending}
            items={modes.map((item) => ({ key: item, label: t(`groups.keyPolicy.modes.${item}`) }))}
            selectedKeys={[mode]}
            size="sm"
            disallowEmptySelection
            onSelectionChange={(keys) => {
              const key = Array.from(keys)[0]
              if (modes.includes(key as GroupKeySelectionMode)) { setMode(key as GroupKeySelectionMode); mutation.reset() }
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </div>
        <p className="rounded-lg bg-surface-2 px-3 py-2 text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.defaultAccess')}</p>
        {mode === 'selected_users' ? <GroupKeyPolicyUsers selectedIds={selectedIds} disabled={mutation.isPending} onChange={(ids) => { setSelectedIds(ids); mutation.reset() }} /> : null}
        <label className="flex items-start gap-2 rounded-lg border border-danger/25 bg-danger/5 px-3 py-2 text-xs leading-5">
          <input type="checkbox" className="mt-0.5 size-4 accent-danger" checked={disableKeys} disabled={mutation.isPending} onChange={(event) => { setDisableKeys(event.target.checked); mutation.reset() }} />
          <span><strong>{t('groups.keyPolicy.disableExistingTitle')}</strong><br />{t('groups.keyPolicy.disableExistingDescription')}</span>
        </label>
        <p className="text-xs leading-5 text-muted-foreground">{t('groups.keyPolicy.revokeHint')}</p>
        {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t('groups.keyPolicy.saveError')}</p> : null}
        {mutation.isSuccess ? <p role="status" className="text-xs text-success">{t('groups.keyPolicy.saved')} {t('groups.keyPolicy.disabledCount', { count: mutation.data.disabled_key_count })} {t('groups.keyPolicy.disabledUserCount', { count: mutation.data.disabled_user_count })}</p> : null}
      </div>
      <div className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] p-4">
        <p className="text-xs text-muted-foreground">{t('groups.keyPolicy.independentSave')}</p>
        <Button type="submit" color="primary" isDisabled={mutation.isPending}>{t(mutation.isPending ? 'groups.keyPolicy.saving' : 'groups.keyPolicy.save')}</Button>
      </div>
    </form>
      <ConfirmModal
        isOpen={confirmRevocation}
        title={t('groups.keyPolicy.confirmDisableTitle')}
        message={t('groups.keyPolicy.confirmDisableDescription')}
        confirmText={t('groups.keyPolicy.confirmDisableAction')}
        cancelText={t('groups.actions.cancel')}
        isLoading={mutation.isPending}
        onCancel={() => setConfirmRevocation(false)}
        onConfirm={() => {
          setConfirmRevocation(false)
          mutation.mutate({ selection_mode: mode, allowed_user_ids: mode === 'selected_users' ? selectedIds : [], disable_keys_losing_access: true })
        }}
      />
    </>
  )
}
