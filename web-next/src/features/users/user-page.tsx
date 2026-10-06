import { useMemo, useState } from 'react'
import { Button, Skeleton } from '@heroui/react'
import { Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTablePagination, DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { useLoadedCursorPagination } from '@/components/data-table/use-table-pagination'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { cn } from '@/lib/utils'
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { useAdminUsers, useUpdateAdminUser } from './user-api'
import { UserDeleteDialog } from './user-delete-dialog'
import { UserEditorSheet } from './user-editor-sheet'
import { userRequestWithStatus } from './user-form-model'
import { UserTable } from './user-table'
import { UserWalletSheet } from './user-wallet-sheet'

type EditorState = AdminUser | 'create' | undefined

export function UserPage() {
  const { t } = useTranslation()
  const [pageSize, setPageSize] = useState(DEFAULT_TABLE_PAGE_SIZE)
  const usersQuery = useAdminUsers(pageSize)
  const pagination = useLoadedCursorPagination({
    availablePageCount: usersQuery.data?.pages.length ?? 1,
    pageSize,
    onPageSizeChange: setPageSize,
  })
  const groupsQuery = useAdminGroupCatalog()
  const toggleMutation = useUpdateAdminUser()
  const [editor, setEditor] = useState<EditorState>()
  const [deleting, setDeleting] = useState<AdminUser>()
  const [togglingId, setTogglingId] = useState<number>()
  const [walletUserId, setWalletUserId] = useState<number>()
  const users = usersQuery.data?.pages[pagination.pageIndex]?.users ?? []
  const groupNames = useMemo(() => new Map((groupsQuery.data ?? []).map((group) => [group.id, group.display_name])), [groupsQuery.data])
  const walletUser = users.find((user) => user.id === walletUserId)

  const toggle = async (user: AdminUser) => {
    setTogglingId(user.id)
    try {
      const status = user.status === 'enabled' ? 'disabled' : 'enabled'
      await toggleMutation.mutateAsync({ id: user.id, body: userRequestWithStatus(user, status) })
    } catch {
      // 服务端状态不变，错误留在列表上下文中供管理员重试。
    } finally {
      setTogglingId(undefined)
    }
  }

  const initialError = usersQuery.isError && usersQuery.data === undefined

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('users.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('users.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={usersQuery.isFetching} onClick={() => void usersQuery.refetch()}><RefreshCw className={cn('size-3.5', usersQuery.isFetching && 'animate-spin')} aria-hidden="true" />{t('users.actions.refresh')}</Button>
          <Button type="button" color="primary" size="sm" onClick={() => setEditor('create')}><Plus className="size-3.5" aria-hidden="true" />{t('users.actions.create')}</Button>
        </div>
      </header>


      <div className="flex items-center justify-between border-t border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('users.pageCount', { count: users.length })}</span>
        <span>{t('users.pages', { count: pagination.currentPage })}</span>
      </div>

      {toggleMutation.isError ? <div role="alert" className="rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('users.error.toggle')}</div> : null}
      {groupsQuery.isError ? <div role="status" className="rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs text-warning">{t('users.error.groups')}</div> : null}

      {usersQuery.isPending ? (
        <div className="grid gap-2" aria-label={t('users.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-20 rounded-xl" />)}</div>
      ) : initialError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h2 className="text-sm font-semibold text-destructive">{t('users.error.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">{t('users.error.body')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void usersQuery.refetch()}>{t('users.actions.retry')}</Button>
        </div>
      ) : <UserTable users={users} groupNames={groupNames} togglingId={togglingId} onDelete={setDeleting} onEdit={setEditor} onToggle={(user) => void toggle(user)} onWallet={(user) => setWalletUserId(user.id)} />}

      {usersQuery.isFetchNextPageError ? <div role="alert" className="flex items-center justify-between gap-3 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive"><span>{t('users.error.more')}</span><Button type="button" size="sm" variant="light" onClick={() => void usersQuery.fetchNextPage()}>{t('users.actions.retry')}</Button></div> : null}
      {!usersQuery.isPending && !initialError ? (
        <DataTablePagination
          currentPage={pagination.currentPage}
          availablePageCount={pagination.availablePageCount}
          pageSize={pagination.pageSize}
          itemCount={users.length}
          hasNextPage={pagination.hasLoadedNextPage || Boolean(usersQuery.hasNextPage)}
          fetching={usersQuery.isFetching}
          onFirstPage={pagination.goToFirstPage}
          onPreviousPage={pagination.goToPreviousPage}
          onPageSelect={pagination.selectPage}
          onNextPage={() => void pagination.goToNextPage(Boolean(usersQuery.hasNextPage), async () => (await usersQuery.fetchNextPage()).isSuccess)}
          onPageSizeChange={pagination.setPageSize}
        />
      ) : null}

      <UserEditorSheet open={editor !== undefined} user={editor === 'create' ? undefined : editor} onOpenChange={(open) => !open && setEditor(undefined)} />
      <UserWalletSheet open={walletUser !== undefined} user={walletUser} onOpenChange={(open) => !open && setWalletUserId(undefined)} />
      <UserDeleteDialog key={deleting?.id ?? 'no-user'} user={deleting} onClose={() => setDeleting(undefined)} />
    </div>
  )
}
