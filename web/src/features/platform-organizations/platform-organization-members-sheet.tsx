import { LoaderCircle, MailPlus, Pencil, RefreshCw, UsersRound } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
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
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="platform-organization-members-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <div className="flex min-w-0 items-center gap-2">
            <SheetTitle className="truncate">{t('platformOrganizations.members.title')}</SheetTitle>
            {organization ? <Badge className="shrink-0">{organization.name}</Badge> : null}
          </div>
          <SheetDescription id="platform-organization-members-description">{t('platformOrganizations.members.description')}</SheetDescription>
        </SheetHeader>
        {organization ? (
          <div className="p-4">
            <div className="mb-3 flex items-center justify-between gap-3 border-b border-[var(--hairline)] pb-3">
              <p className="text-xs text-muted-foreground">{t('platformOrganizations.members.loaded', { count: members.length })}</p>
              <div className="flex gap-1">
                <Button type="button" size="icon-sm" variant="ghost" title={t('platformOrganizations.actions.refresh')} disabled={query.isFetching} onClick={() => void query.refetch()}>
                  <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
                  <span className="sr-only">{t('platformOrganizations.actions.refresh')}</span>
                </Button>
                <Button type="button" size="sm" onClick={() => setInviting(true)}><MailPlus aria-hidden="true" />{t('platformOrganizations.members.invite')}</Button>
              </div>
            </div>
            {query.isPending ? (
              <div className="grid gap-3" role="status">{[0, 1, 2].map((item) => <Skeleton key={item} className="h-20 rounded-md" />)}</div>
            ) : query.isError && members.length === 0 ? (
              <MemberState title={t('platformOrganizations.members.errorTitle')} body={t('platformOrganizations.members.errorBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>{t('platformOrganizations.actions.retry')}</Button>} />
            ) : members.length === 0 ? (
              <MemberState title={t('platformOrganizations.members.emptyTitle')} body={t('platformOrganizations.members.emptyBody')} />
            ) : (
              <div className="divide-y divide-[var(--hairline)]">
                {members.map((member) => <PlatformMemberRow key={member.membership_id} member={member} onEdit={() => setEditing(member)} />)}
              </div>
            )}
            {query.hasNextPage ? (
              <Button type="button" size="sm" variant="secondary" className="mt-4 w-full" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
                {query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
                {t('platformOrganizations.actions.loadMore')}
              </Button>
            ) : null}
          </div>
        ) : null}
        <PlatformOrganizationMemberDialog organizationName={organization?.name ?? ''} member={editing} reason={reason} onOpenChange={(next) => { if (!next) setEditing(undefined) }} />
        <PlatformOrganizationInvitationDialog open={inviting} reason={reason} organizations={organization ? [organization] : []} initialOrganizationId={organization?.id} onOpenChange={setInviting} />
      </SheetContent>
    </Sheet>
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
          <Badge>{t(`platformOrganizations.roles.${member.role}`, { defaultValue: member.role })}</Badge>
          <Badge className={member.status === 'active' ? 'text-success' : member.status === 'removed' ? 'text-destructive' : 'text-warning'}>{t(`platformOrganizations.members.status.${member.status}`, { defaultValue: member.status })}</Badge>
        </div>
        <p className="mt-1 truncate text-xs text-muted-foreground">{member.email ?? t('platformOrganizations.members.noEmail')} · U#{member.user_id} · M#{member.membership_id} · v{member.version}</p>
      </div>
      {editable ? <Button type="button" size="sm" variant="ghost" onClick={onEdit}><Pencil aria-hidden="true" />{t('platformOrganizations.members.edit')}</Button> : <span className="text-[0.6875rem] text-muted-foreground">{t(member.role === 'owner' ? 'platformOrganizations.members.ownerProtected' : 'platformOrganizations.members.removedLocked')}</span>}
    </article>
  )
}

function MemberState({ title, body, action }: { title: string; body: string; action?: React.ReactNode }) {
  return <div className="grid min-h-48 place-items-center py-8 text-center"><div className="max-w-sm"><UsersRound className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
