import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader, Skeleton } from '@heroui/react'
import { LoaderCircle, MailPlus, Pencil, RefreshCw, UsersRound } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { PlatformOrganizationMemberResponse } from '@/lib/api/generated/types.gen'

import { usePlatformOrganizationMembers } from './platform-organization-api'
import { PlatformOrganizationInvitationDialog } from './platform-organization-invitation-dialog'
import { PlatformOrganizationMemberDialog } from './platform-organization-member-dialog'

/** 在企业目录上下文中提供受审计成员读取、邀请与生命周期调配。 */
export function PlatformOrganizationMembersSheet({
  open,
  organization,
  reason,
  initialMembershipId,
  onOpenChange,
}: {
  open: boolean
  organization?: { id: number; name: string }
  reason: string
  initialMembershipId?: number
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const query = usePlatformOrganizationMembers(open && organization ? { organizationId: organization.id, reason } : undefined)
  const members = useMemo(() => query.data?.pages.flatMap((page) => page.members) ?? [], [query.data])
  const [editing, setEditing] = useState<PlatformOrganizationMemberResponse>()
  const [inviting, setInviting] = useState(false)

  useEffect(() => {
    if (!open) {
      setEditing(undefined)
      setInviting(false)
      return
    }
    if (initialMembershipId && !editing) {
      setEditing(members.find((member) => member.membership_id === initialMembershipId))
    }
  }, [editing, initialMembershipId, members, open])

  useEffect(() => {
    if (!editing) return
    const refreshed = members.find((member) => member.membership_id === editing.membership_id)
    if (refreshed && refreshed.version !== editing.version) setEditing(refreshed)
  }, [editing, members])

  return (
    <Drawer
      aria-describedby="platform-organization-members-description"
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <div className="flex min-w-0 items-center gap-2">
                <h2 className="truncate text-base font-medium text-foreground">{t('platformOrganizations.members.title')}</h2>
                {organization ? <Chip className="shrink-0" size="sm" variant="flat">{organization.name}</Chip> : null}
              </div>
              <p className="text-sm text-muted-foreground" id="platform-organization-members-description">{t('platformOrganizations.members.description')}</p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              {organization ? (
                <div className="overflow-y-auto p-4">
                  <div className="mb-3 flex items-center justify-between gap-3 border-b border-[var(--hairline)] pb-3">
                    <p className="text-xs text-muted-foreground">{t('platformOrganizations.members.loaded', { count: members.length })}</p>
                    <div className="flex gap-1">
                      <Button isIconOnly type="button" size="sm" title={t('platformOrganizations.actions.refresh')} variant="light" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
                        <RefreshCw className={cn('size-3.5', query.isFetching && 'animate-spin')} aria-hidden="true" />
                        <span className="sr-only">{t('platformOrganizations.actions.refresh')}</span>
                      </Button>
                      <Button type="button" color="primary" size="sm" onClick={() => setInviting(true)}><MailPlus className="size-3.5" aria-hidden="true" />{t('platformOrganizations.members.invite')}</Button>
                    </div>
                  </div>
                  {query.isPending ? (
                    <div className="grid gap-3" role="status">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-md" />)}</div>
                  ) : query.isError && members.length === 0 ? (
                    <MemberState title={t('platformOrganizations.members.errorTitle')} body={t('platformOrganizations.members.errorBody')} action={<Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}>{t('platformOrganizations.actions.retry')}</Button>} />
                  ) : members.length === 0 ? (
                    <MemberState title={t('platformOrganizations.members.emptyTitle')} body={t('platformOrganizations.members.emptyBody')} />
                  ) : (
                    <div className="divide-y divide-[var(--hairline)]">
                      {members.map((member) => <PlatformMemberRow key={member.membership_id} member={member} onEdit={() => setEditing(member)} />)}
                    </div>
                  )}
                  {query.hasNextPage ? (
                    <Button type="button" size="sm" variant="bordered" className="mt-4 w-full" isDisabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
                      {query.isFetchingNextPage ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : null}
                      {t('platformOrganizations.actions.loadMore')}
                    </Button>
                  ) : null}
                </div>
              ) : null}
            </DrawerBody>
          </>
        )}
      </DrawerContent>
      <PlatformOrganizationMemberDialog organizationName={organization?.name ?? ''} member={editing} reason={reason} onOpenChange={(next) => { if (!next) setEditing(undefined) }} />
      <PlatformOrganizationInvitationDialog open={inviting} reason={reason} organizations={organization ? [organization] : []} initialOrganizationId={organization?.id} onOpenChange={setInviting} />
    </Drawer>
  )
}

function PlatformMemberRow({ member, onEdit }: { member: PlatformOrganizationMemberResponse; onEdit: () => void }) {
  const { t } = useTranslation()
  const editable = member.role !== 'owner' && member.status !== 'removed'
  return (
    <article className="grid gap-3 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="min-w-0">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate text-sm font-medium">{member.username}</span>
          <Chip size="sm" variant="flat">{t(`platformOrganizations.roles.${member.role}`, { defaultValue: member.role })}</Chip>
          <Chip className={member.status === 'active' ? 'text-success' : member.status === 'removed' ? 'text-destructive' : 'text-warning'} size="sm" variant="flat">{t(`platformOrganizations.members.status.${member.status}`, { defaultValue: member.status })}</Chip>
        </div>
        <p className="mt-1 truncate text-xs text-muted-foreground">{member.email ?? t('platformOrganizations.members.noEmail')} · U#{member.user_id} · M#{member.membership_id} · v{member.version}</p>
      </div>
      {editable ? <Button type="button" size="sm" variant="light" onClick={onEdit}><Pencil className="size-3.5" aria-hidden="true" />{t('platformOrganizations.members.edit')}</Button> : <span className="text-[0.6875rem] text-muted-foreground">{t(member.role === 'owner' ? 'platformOrganizations.members.ownerProtected' : 'platformOrganizations.members.removedLocked')}</span>}
    </article>
  )
}

function MemberState({ title, body, action }: { title: string; body: string; action?: React.ReactNode }) {
  return <div className="grid min-h-48 place-items-center py-8 text-center"><div className="max-w-sm"><UsersRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
