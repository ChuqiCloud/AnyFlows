import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button, Input } from '@heroui/react'
import { useAdminUsers } from '@/features/users/user-api'

type GroupKeyPolicyUsersProps = {
  selectedIds: number[]
  disabled: boolean
  onChange: (ids: number[]) => void
}

/** 已授权用户独立于分页目录保存，目录未读取的 ID 仍可明确查看与移除。 */
export function GroupKeyPolicyUsers({ selectedIds, disabled, onChange }: GroupKeyPolicyUsersProps) {
  const { t } = useTranslation()
  const [search, setSearch] = useState('')
  const usersQuery = useAdminUsers()
  const users = useMemo(() => usersQuery.data?.pages.flatMap((page) => page.users) ?? [], [usersQuery.data])
  const query = search.trim().toLocaleLowerCase()
  const matches = users.filter((user) => !selectedIds.includes(user.id)
    && `${user.username} ${user.email ?? ''} ${user.id}`.toLocaleLowerCase().includes(query))
  const userLabel = (id: number) => {
    const user = users.find((candidate) => candidate.id === id)
    return user ? `${user.username}${user.email ? ` (${user.email})` : ''} · #${id}` : t('groups.keyPolicy.userFallback', { id })
  }

  return (
    <div className="grid min-w-0 gap-3">
      <div className="grid gap-1.5">
        <label className="text-xs font-medium" htmlFor="group-key-user-search">{t('groups.keyPolicy.searchLabel')}</label>
        <Input size="sm" aria-label={t('groups.keyPolicy.searchLabel')} id="group-key-user-search" value={search} isDisabled={disabled} placeholder={t('groups.keyPolicy.searchPlaceholder')} onChange={(event) => setSearch(event.target.value)} />
        <p className="text-xs text-muted-foreground">{t('groups.keyPolicy.loadedScope', { count: users.length })}</p>
      </div>

      <div className="grid gap-2" aria-label={t('groups.keyPolicy.selectedUsers')}>
        <p className="text-xs font-medium">{t('groups.keyPolicy.selectedCount', { count: selectedIds.length })}</p>
        {selectedIds.map((id) => (
          <div key={id} className="flex min-w-0 items-center justify-between gap-2 rounded-lg border border-[var(--hairline)] px-3 py-2">
            <span className="min-w-0 break-all text-xs">{userLabel(id)}</span>
            <Button type="button" size="sm" variant="light" isDisabled={disabled} aria-label={t('groups.keyPolicy.removeUser', { user: userLabel(id) })} onClick={() => onChange(selectedIds.filter((selected) => selected !== id))}>{t('groups.keyPolicy.remove')}</Button>
          </div>
        ))}
      </div>

      <div className="grid max-h-56 gap-1 overflow-y-auto rounded-lg border border-[var(--hairline)] p-2" aria-label={t('groups.keyPolicy.availableUsers')}>
        {matches.map((user) => (
          <button key={user.id} type="button" disabled={disabled || selectedIds.length >= 1000} className="rounded-md px-2 py-2 text-left text-xs hover:bg-surface-2 disabled:opacity-50" aria-label={t('groups.keyPolicy.addUser', { user: userLabel(user.id) })} onClick={() => onChange([...selectedIds, user.id])}>
            <span className="block break-all font-medium">{user.username} · #{user.id}</span>
            {user.email ? <span className="block break-all text-muted-foreground">{user.email}</span> : null}
          </button>
        ))}
        {!usersQuery.isPending && matches.length === 0 ? <p className="px-2 py-3 text-xs text-muted-foreground">{t('groups.keyPolicy.noUsers')}</p> : null}
        {usersQuery.isPending ? <p role="status" className="px-2 py-3 text-xs text-muted-foreground">{t('groups.keyPolicy.loadingUsers')}</p> : null}
      </div>
      {usersQuery.isError ? (
        <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('groups.keyPolicy.usersError')}</span>
          <Button type="button" size="sm" variant="flat" isDisabled={usersQuery.isFetching} onClick={() => void usersQuery.refetch()}>{t('groups.actions.retry')}</Button>
        </div>
      ) : null}
      {usersQuery.hasNextPage ? <Button type="button" size="sm" variant="flat" isDisabled={disabled || usersQuery.isFetchingNextPage} onClick={() => void usersQuery.fetchNextPage()}>{t(usersQuery.isFetchingNextPage ? 'groups.keyPolicy.loadingUsers' : 'groups.keyPolicy.loadMoreUsers')}</Button> : null}
    </div>
  )
}
