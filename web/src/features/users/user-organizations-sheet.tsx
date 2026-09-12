import { Building2, LoaderCircle, MailPlus, Pencil } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import {
  usePlatformOrganizations,
  usePlatformUserOrganizations,
} from '@/features/platform-organizations/platform-organization-api'
import { PlatformOrganizationInvitationDialog } from '@/features/platform-organizations/platform-organization-invitation-dialog'
import { PlatformOrganizationMemberDialog } from '@/features/platform-organizations/platform-organization-member-dialog'
import type { AdminUser, PlatformOrganizationMemberResponse, PlatformUserOrganizationSummaryResponse } from '@/lib/api/generated/types.gen'

type UserOrganizationsSheetProps = {
  open: boolean
  user?: AdminUser
  onOpenChange: (open: boolean) => void
}

/** 管理员打开目标用户后直接读取企业归属。 */
export function UserOrganizationsSheet({ open, user, onOpenChange }: UserOrganizationsSheetProps) {
  const { i18n, t } = useTranslation()
  const [inviteOpen, setInviteOpen] = useState(false)
  const [editing, setEditing] = useState<PlatformOrganizationMemberResponse>()
  const query = usePlatformUserOrganizations(open ? user?.id : undefined)
  const directoryQuery = usePlatformOrganizations(inviteOpen, 100)
  const organizations = query.data?.pages.flatMap((page) => page.organizations) ?? []
  const directory = directoryQuery.data?.pages.flatMap((page) => page.organizations) ?? []
  const timeFormat = new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
  })

  useEffect(() => {
    setInviteOpen(false)
    setEditing(undefined)
  }, [open, user?.id])

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-y-auto data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="user-organizations-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <div className="flex min-w-0 items-center gap-2">
            <SheetTitle className="truncate">{t('users.organizations.title')}</SheetTitle>
            {user ? <Badge className="shrink-0">{user.username}</Badge> : null}
          </div>
          <SheetDescription id="user-organizations-description">
            {t('users.organizations.description')}
          </SheetDescription>
        </SheetHeader>
        {user ? (
          <div className="p-4">
            <div className="flex items-start justify-between gap-3 border-b border-[var(--hairline)] py-4">
              <div>
                <p className="text-xs font-medium">{t('users.organizations.assignTitle')}</p>
                <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('users.organizations.assignHint')}</p>
              </div>
              <Button type="button" size="sm" disabled={!user.email} onClick={() => setInviteOpen(true)}><MailPlus aria-hidden="true" />{t('users.organizations.assign')}</Button>
            </div>

            {query.isPending ? (
              <div className="py-8 text-center text-xs text-muted-foreground">{t('users.organizations.loading')}</div>
            ) : query.isError && query.data === undefined ? (
              <MembershipState
                title={t('users.organizations.errorTitle')}
                body={t('users.organizations.errorBody')}
                action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>{t('users.actions.retry')}</Button>}
              />
            ) : organizations.length === 0 ? (
              <MembershipState title={t('users.organizations.emptyTitle')} body={t('users.organizations.emptyBody')} />
            ) : (
              <div className="divide-y divide-[var(--hairline)]">
                {organizations.map((organization) => (
                  <UserOrganizationRow key={organization.membership_id} organization={organization} timeFormat={timeFormat} onEdit={() => setEditing({
                    membership_id: organization.membership_id,
                    organization_id: organization.organization_id,
                    user_id: user.id,
                    username: user.username,
                    email: user.email,
                    role: organization.role,
                    team_id: null,
                    status: organization.membership_status,
                    joined_at: organization.joined_at,
                    suspended_at: null,
                    removed_at: null,
                    version: organization.version,
                  })} />
                ))}
              </div>
            )}

            {query.isFetchNextPageError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('users.organizations.moreError')}</p> : null}
            {query.hasNextPage ? (
              <Button
                type="button"
                size="sm"
                variant="secondary"
                className="mt-4 w-full"
                disabled={query.isFetchingNextPage}
                onClick={() => void query.fetchNextPage()}
              >
                {query.isFetchingNextPage ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
                {t('users.organizations.loadMore')}
              </Button>
            ) : null}
          </div>
        ) : null}
        <PlatformOrganizationMemberDialog organizationName={organizations.find((organization) => organization.membership_id === editing?.membership_id)?.organization_name ?? ''} member={editing} onOpenChange={(next) => { if (!next) setEditing(undefined) }} />
        <PlatformOrganizationInvitationDialog
          open={inviteOpen}
          organizations={directory.map((organization) => ({ id: organization.id, name: organization.name }))}
          initialEmail={user?.email ?? undefined}
          lockEmail
          hasMoreOrganizations={directoryQuery.hasNextPage}
          loadingMoreOrganizations={directoryQuery.isFetchingNextPage}
          onLoadMoreOrganizations={() => void directoryQuery.fetchNextPage()}
          onOpenChange={setInviteOpen}
        />
      </SheetContent>
    </Sheet>
  )
}

function UserOrganizationRow({
  organization,
  timeFormat,
  onEdit,
}: {
  organization: PlatformUserOrganizationSummaryResponse
  timeFormat: Intl.DateTimeFormat
  onEdit: () => void
}) {
  const { t } = useTranslation()
  return (
    <article className="py-4 first:pt-0 last:pb-0">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold">{organization.organization_name}</h3>
          <p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">
            {organization.organization_slug} · #{organization.organization_id}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-1">
          <Badge>{t(`platformOrganizations.roles.${organization.role}`, { defaultValue: organization.role })}</Badge>
          <Badge>{t(`platformOrganizations.membershipStatus.${organization.membership_status}`, { defaultValue: organization.membership_status })}</Badge>
        </div>
      </div>
      <dl className="mt-3 grid grid-cols-2 gap-3 text-xs">
        <div><dt className="text-muted-foreground">{t('users.organizations.entitlement')}</dt><dd className="mt-1">{t(`platformOrganizations.entitlementStatus.${organization.entitlement_status}`, { defaultValue: organization.entitlement_status })}</dd></div>
        <div><dt className="text-muted-foreground">{t('users.organizations.joinedAt')}</dt><dd className="mt-1">{timeFormat.format(organization.joined_at * 1_000)}</dd></div>
        <div className="col-span-2"><dt className="text-muted-foreground">{t('users.organizations.membershipId')}</dt><dd className="mt-1 font-mono">#{organization.membership_id} · v{organization.version}</dd></div>
      </dl>
      {organization.membership_status !== 'removed' && organization.role !== 'owner' ? <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={onEdit}><Pencil aria-hidden="true" />{t('users.organizations.manageMembership')}</Button> : null}
    </article>
  )
}

function MembershipState({ title, body, action }: { title: string; body: string; action?: React.ReactNode }) {
  return (
    <div className="grid min-h-48 place-items-center py-8 text-center">
      <div className="max-w-sm">
        <Building2 className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h3 className="mt-3 text-sm font-semibold">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
