import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button, Input } from '@heroui/react'
import { useAdminGroupCatalog } from './group-api'

type GroupKeyPolicyGroupsProps = {
  selectedIds: number[]
  disabled: boolean
  onChange: (ids: number[]) => void
}

/** 已授权默认分组独立于完整目录保存，目录未读取的 ID 仍可明确查看与移除。 */
export function GroupKeyPolicyGroups({ selectedIds, disabled, onChange }: GroupKeyPolicyGroupsProps) {
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const groupsQuery = useAdminGroupCatalog()
  const groups = groupsQuery.data ?? []
  const query = search.trim().toLocaleLowerCase()
  const matches = groups.filter((group) => !selectedIds.includes(group.id)
    && `${group.display_name} ${group.name} ${group.id}`.toLocaleLowerCase().includes(query))
  const groupLabel = (id: number) => {
    const group = groups.find((candidate) => candidate.id === id)
    return group ? `${group.display_name} (${group.name}) · #${id}` : t('groups.keyPolicy.groupFallback', { id })
  }

  return (
    <div className="grid min-w-0 gap-3">
      <div className="grid gap-1.5">
        <label className="text-xs font-medium" htmlFor="group-key-policy-group-search">{t('groups.keyPolicy.searchLabel')}</label>
        <Input size="sm" aria-label={t('groups.keyPolicy.searchLabel')} id="group-key-policy-group-search" value={search} isDisabled={disabled} placeholder={t('groups.keyPolicy.searchPlaceholder')} onChange={(event) => setSearch(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t('groups.keyPolicy.loadedScope', { count: groups.length })}</p>
      </div>

      <div className="grid gap-2" aria-label={t('groups.keyPolicy.selectedGroups')}>
        <p className="text-xs font-medium">{t('groups.keyPolicy.selectedCount', { count: selectedIds.length })}</p>
        {selectedIds.map((id) => (
          <div key={id} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-[var(--hairline)] px-3 py-2">
            <span className="min-w-0 break-all text-xs">{groupLabel(id)}</span>
            <Button type="button" size="sm" variant="light" isDisabled={disabled} aria-label={t('groups.keyPolicy.removeGroup', { group: groupLabel(id) })} onClick={() => onChange(selectedIds.filter((selected) => selected !== id))}>{t('groups.keyPolicy.remove')}</Button>
          </div>
        ))}
      </div>

      <div className="grid max-h-56 gap-1 overflow-y-auto rounded-lg border border-[var(--hairline)] p-2" aria-label={t('groups.keyPolicy.availableGroups')}>
        {matches.map((group) => (
          <button key={group.id} type="button" disabled={disabled || selectedIds.length >= 1000} className="rounded-md px-2 py-2 text-left text-xs hover:bg-surface-2 disabled:opacity-50" aria-label={t('groups.keyPolicy.addGroup', { group: groupLabel(group.id) })} onClick={() => onChange([...selectedIds, group.id])}>
            <span className="block break-all font-medium">{group.display_name} · #{group.id}</span>
            <span className="block break-all text-muted-foreground">{group.name}</span>
          </button>
        ))}
        {!groupsQuery.isPending && matches.length === 0 ? <p className="px-2 py-3 text-xs text-muted-foreground">{t('groups.keyPolicy.noGroups')}</p> : null}
        {groupsQuery.isPending ? <p role="status" className="px-2 py-3 text-xs text-muted-foreground">{t('groups.keyPolicy.loadingGroups')}</p> : null}
      </div>
      {groupsQuery.isError ? (
        <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('groups.keyPolicy.groupsError')}</span>
          <Button type="button" size="sm" variant="flat" isDisabled={groupsQuery.isFetching} onClick={() => void groupsQuery.refetch()}>{t('groups.actions.retry')}</Button>
        </div>
      ) : null}
    </div>
  )
}
