import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input } from '@heroui/react'
import { Building2, LoaderCircle, MailPlus, Pencil, Search } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import {
  isValidPlatformOrganizationReason,
  usePlatformOrganizations,
  usePlatformUserOrganizations,
  type PlatformUserOrganizationAccess,
} from '@/features/platform-organizations/platform-organization-api'
import { PlatformOrganizationInvitationDialog } from '@/features/platform-organizations/platform-organization-invitation-dialog'
import { PlatformOrganizationMemberDialog } from '@/features/platform-organizations/platform-organization-member-dialog'
import type { AdminUser, PlatformOrganizationMemberResponse, PlatformUserOrganizationSummaryResponse } from '@/lib/api/generated/types.gen'

type UserOrganizationsSheetProps = {
  open: boolean
  user?: AdminUser
  onOpenChange: (open: boolean) => void
}

/** 按管理员明确选择的用户和临时理由读取企业归属，不预取其他用户。 */
export function UserOrganizationsSheet({ open, user, onOpenChange }: UserOrganizationsSheetProps) {
  const { i18n, t } = useTranslation()
  const [reason, setReason] = useState('')
  const [access, setAccess] = useState<PlatformUserOrganizationAccess>()
  const [inviteOpen, setInviteOpen] = useState(false)
  const [editing, setEditing] = useState<PlatformOrganizationMemberResponse>()
  const query = usePlatformUserOrganizations(open ? access : undefined)
  const directoryQuery = usePlatformOrganizations(inviteOpen && access ? { reason: access.reason } : undefined, 100)
  const organizations = query.data?.pages.flatMap((page) => page.organizations) ?? []
  const directory = directoryQuery.data?.pages.flatMap((page) => page.organizations) ?? []
  const validReason = isValidPlatformOrganizationReason(reason)
  const timeFormat = new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', {
    dateStyle: 'medium',
    timeStyle: 'short',
  })

  useEffect(() => {
    setReason('')
    setAccess(undefined)
    setInviteOpen(false)
    setEditing(undefined)
  }, [open, user?.id])

  const load = (event: FormEvent) => {
    event.preventDefault()
    if (!user || !validReason) return
    if (access?.userId === user.id && access.reason === reason) {
      void query.refetch()
      return
    }
    setAccess({ userId: user.id, reason })
  }

  return (
    <Drawer
      aria-describedby="user-organizations-description"
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
                <h2 className="truncate text-base font-medium text-foreground">{t('users.organizations.title')}</h2>
                {user ? <Chip className="shrink-0" size="sm" variant="flat">{user.username}</Chip> : null}
              </div>
              <p className="text-sm text-muted-foreground" id="user-organizations-description">
                {t('users.organizations.description')}
              </p>
            </DrawerHeader>
            {user ? (
              <DrawerBody className="min-h-0 flex-1 gap-0 overflow-y-auto p-4">
                <form className="border-b border-[var(--hairline)] pb-4" onSubmit={load}>
                  {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有层级。 */}
                  <label className="text-xs font-medium leading-none text-foreground" htmlFor="user-organizations-reason">{t('users.organizations.reasonLabel')}</label>
                  <Input
                    autoComplete="off"
                    className="mt-2"
                    id="user-organizations-reason"
                    isInvalid={reason.length > 0 && !validReason}
                    maxLength={255}
                    placeholder={t('users.organizations.reasonPlaceholder')}
                    size="sm"
                    value={reason}
                    aria-describedby="user-organizations-reason-hint"
                    onValueChange={setReason}
                  />
              <p id="user-organizations-reason-hint" className="mt-1.5 text-[0.6875rem] leading-4 text-muted-foreground">
                {reason.length > 0 && !validReason
                  ? t('users.organizations.reasonInvalid')
                  : t('users.organizations.reasonHint')}
              </p>
              <Button type="submit" className="mt-3" isDisabled={!validReason || query.isFetching} size="sm">
                {query.isFetching && !query.data ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Search className="size-3.5" aria-hidden="true" />}
                {t('users.organizations.load')}
              </Button>
            </form>

            {access ? (
              <div className="flex items-start justify-between gap-3 border-b border-[var(--hairline)] py-4">
                <div>
                  <p className="text-xs font-medium">{t('users.organizations.assignTitle')}</p>
                  <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('users.organizations.assignHint')}</p>
                </div>
                <Button type="button" isDisabled={!user.email} size="sm" onClick={() => setInviteOpen(true)}><MailPlus className="size-3.5" aria-hidden="true" />{t('users.organizations.assign')}</Button>
              </div>
            ) : null}

            {!access ? (
              <MembershipState title={t('users.organizations.initialTitle')} body={t('users.organizations.initialBody')} />
            ) : query.isPending ? (
              <div className="py-8 text-center text-xs text-muted-foreground">{t('users.organizations.loading')}</div>
            ) : query.isError && query.data === undefined ? (
              <MembershipState
                title={t('users.organizations.errorTitle')}
                body={t('users.organizations.errorBody')}
                action={<Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}>{t('users.actions.retry')}</Button>}
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
                className="mt-4 w-full"
                isDisabled={query.isFetchingNextPage}
                size="sm"
                variant="bordered"
                onClick={() => void query.fetchNextPage()}
              >
                {query.isFetchingNextPage ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : null}
                {t('users.organizations.loadMore')}
              </Button>
            ) : null}
              </DrawerBody>
            ) : null}
            <PlatformOrganizationMemberDialog organizationName={organizations.find((organization) => organization.membership_id === editing?.membership_id)?.organization_name ?? ''} member={editing} reason={access?.reason ?? ''} onOpenChange={(next) => { if (!next) setEditing(undefined) }} />
            <PlatformOrganizationInvitationDialog
              open={inviteOpen}
              reason={access?.reason ?? ''}
              organizations={directory.map((organization) => ({ id: organization.id, name: organization.name }))}
              initialEmail={user?.email ?? undefined}
              lockEmail
              hasMoreOrganizations={directoryQuery.hasNextPage}
              loadingMoreOrganizations={directoryQuery.isFetchingNextPage}
              onLoadMoreOrganizations={() => void directoryQuery.fetchNextPage()}
              onOpenChange={setInviteOpen}
            />
          </>
        )}
      </DrawerContent>
    </Drawer>
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
          <Chip size="sm" variant="flat">{t(`platformOrganizations.roles.${organization.role}`, { defaultValue: organization.role })}</Chip>
          <Chip size="sm" variant="flat">{t(`platformOrganizations.membershipStatus.${organization.membership_status}`, { defaultValue: organization.membership_status })}</Chip>
        </div>
      </div>
      <dl className="mt-3 grid grid-cols-2 gap-3 text-xs">
        <div><dt className="text-muted-foreground">{t('users.organizations.entitlement')}</dt><dd className="mt-1">{t(`platformOrganizations.entitlementStatus.${organization.entitlement_status}`, { defaultValue: organization.entitlement_status })}</dd></div>
        <div><dt className="text-muted-foreground">{t('users.organizations.joinedAt')}</dt><dd className="mt-1">{timeFormat.format(organization.joined_at * 1_000)}</dd></div>
        <div className="col-span-2"><dt className="text-muted-foreground">{t('users.organizations.membershipId')}</dt><dd className="mt-1 font-mono">#{organization.membership_id} · v{organization.version}</dd></div>
      </dl>
      {organization.membership_status !== 'removed' && organization.role !== 'owner' ? <Button type="button" className="mt-3" size="sm" variant="bordered" onClick={onEdit}><Pencil className="size-3.5" aria-hidden="true" />{t('users.organizations.manageMembership')}</Button> : null}
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
