import {
  Building2,
  MailPlus,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  RotateCw,
  Send,
  ShieldX,
  Trash2,
  UsersRound,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { useOrganizationTeamCatalog } from '@/features/organization-budgets/organization-budget-api'
import type { OrganizationInvitationResponse, OrganizationMembershipResponse, OrganizationOverviewResponse, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  OrganizationPageBoundary,
  OrganizationPageHeader,
  useOrganizationPageState,
} from '@/features/organization-console/organization-page-shell'
import { OrganizationBatchInvitationSheet } from './organization-batch-invitation-sheet'
import { OrganizationInvitationEditorSheet } from './organization-invitation-editor-sheet'
import { OrganizationInvitationIssuedDialog } from './organization-invitation-issued-dialog'
import {
  organizationMemberErrorCode,
  useDisableOrganizationTeam,
  useOrganizationInvitations,
  useOrganizationMembers,
  useOrganizationTeams,
  useResendOrganizationInvitation,
  useRevokeOrganizationInvitation,
} from './organization-member-api'
import { OrganizationMemberEditorSheet } from './organization-member-editor-sheet'
import { OrganizationTeamEditorSheet } from './organization-team-editor-sheet'
import { useOrganizationCustomRoles } from '@/features/organization-roles/organization-custom-role-api'

const MEMBERS_MANAGE = 'organization.members.manage'
const TEAMS_MANAGE = 'organization.teams.manage'
const ROLES_ASSIGN = 'organization.roles.assign'
type MemberTab = 'invitations' | 'members' | 'teams'

/** 企业成员域集中管理成员、团队和邀请，所有写入均由服务端权限与 CAS 再校验。 */
export function OrganizationMemberPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={UsersRound} title={t('organizationConsole.members.title')} subtitle={t('organizationConsole.members.subtitle')} />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => <OrganizationMemberWorkspace key={organizationId} organizationId={organizationId} overview={overview} />}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationMemberWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const [tab, setTab] = useState<MemberTab>('members')
  const canManageMembers = overview.permissions.includes(MEMBERS_MANAGE)
  const canManageTeams = overview.permissions.includes(TEAMS_MANAGE)
  const canAssignCustomRoles = overview.permissions.includes(ROLES_ASSIGN)
  return (
    <>
      <section className="grid gap-3 sm:grid-cols-3">
        <SummaryMetric icon={UsersRound} label={t('organizationConsole.members.summary.seats')} value={`${overview.entitlement.active_seats} / ${overview.entitlement.seat_limit}`} />
        <SummaryMetric icon={Building2} label={t('organizationConsole.members.summary.teams')} value={`${overview.entitlement.active_teams} / ${overview.entitlement.team_limit}`} />
        <SummaryMetric icon={ShieldX} label={t('organizationConsole.members.summary.role')} value={t(`organizationConsole.roles.${overview.role}`)} />
      </section>

      <div className="grid grid-cols-3 gap-1 rounded-lg bg-surface-2 p-1" role="tablist" aria-label={t('organizationConsole.members.tabs.label')}>
        {(['members', 'teams', 'invitations'] as const).map((value) => (
          <Button key={value} type="button" size="sm" variant="ghost" role="tab" aria-selected={tab === value} className={cn('justify-center', tab === value && 'bg-background shadow-xs hover:bg-background')} onClick={() => setTab(value)}>
            {t(`organizationConsole.members.tabs.${value}`)}
          </Button>
        ))}
      </div>

      {tab === 'members' ? <MembersPanel organizationId={organizationId} organizationName={overview.name} canManage={canManageMembers} canAssignCustomRoles={canAssignCustomRoles} customRolesEnabled={overview.entitlement.custom_roles_enabled} /> : null}
      {tab === 'teams' ? <TeamsPanel organizationId={organizationId} organizationName={overview.name} canManage={canManageTeams} enabled={overview.entitlement.team_management_enabled} /> : null}
      {tab === 'invitations' ? <InvitationsPanel organizationId={organizationId} organizationName={overview.name} canManage={canManageMembers} /> : null}
    </>
  )
}

function MembersPanel({ organizationId, organizationName, canManage, canAssignCustomRoles, customRolesEnabled }: { organizationId: number; organizationName: string; canManage: boolean; canAssignCustomRoles: boolean; customRolesEnabled: boolean }) {
  const { t } = useTranslation()
  const query = useOrganizationMembers(organizationId)
  const teamCatalog = useOrganizationTeamCatalog(organizationId, canManage)
  const customRoleCatalog = useOrganizationCustomRoles(organizationId, customRolesEnabled && canAssignCustomRoles)
  const [editing, setEditing] = useState<OrganizationMembershipResponse>()
  const members = query.data?.pages.flatMap((page) => page.members) ?? []
  const teamNames = useMemo(() => new Map((teamCatalog.data ?? []).map((team) => [team.id, team.name])), [teamCatalog.data])
  const customRoles = useMemo(() => customRoleCatalog.data?.pages.flatMap((page) => page.roles) ?? [], [customRoleCatalog.data])
  const customRoleNames = useMemo(() => new Map(customRoles.map((role) => [role.role_key, role.name])), [customRoles])
  return (
    <Card className="overflow-hidden">
      <PanelHeader icon={UsersRound} title={t('organizationConsole.members.list.title')} subtitle={t('organizationConsole.members.list.subtitle')} count={members.length} refreshing={query.isFetching} onRefresh={() => void query.refetch()} />
      <CardContent className="p-0">
        <PagedContent query={query} items={members} emptyIcon={UsersRound} emptyTitle={t('organizationConsole.members.list.emptyTitle')} emptyBody={t('organizationConsole.members.list.emptyBody')}>
          <div className="divide-y divide-[var(--hairline)]">
            {members.map((member) => (
              <article key={member.id} className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-center">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">{t('organizationConsole.members.memberLabel', { id: member.user_id })}</span>
                    <Badge className={member.role === 'owner' ? 'text-warning' : undefined}>{t(`organizationConsole.roles.${member.role}`)}</Badge>
                    <Badge className={member.status === 'active' ? 'text-success' : member.status === 'removed' ? 'text-destructive' : 'text-warning'}>{t(`organizationConsole.memberStatus.${member.status}`)}</Badge>
                    {member.custom_role_key ? <Badge className="text-brand">{customRoleNames.get(member.custom_role_key) ?? t('organizationConsole.members.customRoles.assigned')}</Badge> : null}
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">{member.team_id ? teamNames.get(member.team_id) ?? t('organizationConsole.members.teamLabel', { id: member.team_id }) : t('organizationConsole.members.noTeam')} · {t('organizationConsole.members.membershipLabel', { id: member.id })}</p>
                </div>
                <span className="text-xs text-muted-foreground tabular-nums">v{member.version}</span>
                {canManage ? <Button type="button" size="sm" variant="ghost" disabled={member.role === 'owner'} onClick={() => setEditing(member)}><Pencil aria-hidden="true" />{t('organizationConsole.actions.edit')}</Button> : null}
              </article>
            ))}
          </div>
        </PagedContent>
      </CardContent>
      <OrganizationMemberEditorSheet organizationId={organizationId} organizationName={organizationName} member={editing} teams={teamCatalog.data ?? []} customRoles={customRoles} customRolesReady={customRoleCatalog.isSuccess} customRolesEnabled={customRolesEnabled} canAssignCustomRoles={canAssignCustomRoles} open={editing !== undefined} onOpenChange={(open) => { if (!open) setEditing(undefined) }} />
    </Card>
  )
}

function TeamsPanel({ organizationId, organizationName, canManage, enabled }: { organizationId: number; organizationName: string; canManage: boolean; enabled: boolean }) {
  const { t } = useTranslation()
  const query = useOrganizationTeams(organizationId)
  const disableMutation = useDisableOrganizationTeam(organizationId)
  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<OrganizationTeamResponse>()
  const [disabling, setDisabling] = useState<OrganizationTeamResponse>()
  const teams = query.data?.pages.flatMap((page) => page.teams) ?? []
  const disable = async () => {
    if (!disabling) return
    try {
      await disableMutation.mutateAsync({ teamId: disabling.id, expectedVersion: disabling.version })
      setDisabling(undefined)
    } catch {
      // 活动成员或 CAS 冲突时保留确认目标并展示错误。
    }
  }
  return (
    <Card className="overflow-hidden">
      <PanelHeader icon={Building2} title={t('organizationConsole.teams.list.title')} subtitle={t('organizationConsole.teams.list.subtitle')} count={teams.length} refreshing={query.isFetching} onRefresh={() => void query.refetch()} actions={canManage && enabled ? <Button type="button" size="sm" onClick={() => { setEditing(undefined); setEditorOpen(true) }}><Plus aria-hidden="true" />{t('organizationConsole.teams.actions.create')}</Button> : undefined} />
      {!enabled ? <div className="border-b border-[var(--hairline)] bg-warning/8 px-4 py-3 text-xs text-warning">{t('organizationConsole.teams.disabledByEntitlement')}</div> : null}
      <CardContent className="p-0">
        <PagedContent query={query} items={teams} emptyIcon={Building2} emptyTitle={t('organizationConsole.teams.list.emptyTitle')} emptyBody={t('organizationConsole.teams.list.emptyBody')}>
          <div className="divide-y divide-[var(--hairline)]">
            {teams.map((team) => (
              <article key={team.id} className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2"><span className="text-sm font-medium">{team.name}</span><Badge className={team.status === 'active' ? 'text-success' : 'text-muted-foreground'}>{t(`organizationConsole.teamStatus.${team.status}`)}</Badge></div>
                  <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">{team.description || t('organizationConsole.teams.noDescription')} · #{team.id} · v{team.version}</p>
                </div>
                {canManage && team.status === 'active' ? (
                  <div className="flex gap-1">
                    <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.edit')} onClick={() => { setEditing(team); setEditorOpen(true) }}><Pencil aria-hidden="true" /><span className="sr-only">{t('organizationConsole.actions.edit')}</span></Button>
                    <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.teams.actions.disable')} onClick={() => setDisabling(team)}><Trash2 aria-hidden="true" /><span className="sr-only">{t('organizationConsole.teams.actions.disable')}</span></Button>
                  </div>
                ) : null}
              </article>
            ))}
          </div>
        </PagedContent>
      </CardContent>
      <OrganizationTeamEditorSheet organizationId={organizationId} organizationName={organizationName} team={editing} open={editorOpen} onOpenChange={(open) => { setEditorOpen(open); if (!open) setEditing(undefined) }} />
      <AlertDialog open={disabling !== undefined} onOpenChange={(open) => { if (!open) setDisabling(undefined) }}>
        <AlertDialogContent>
          <AlertDialogHeader><AlertDialogTitle>{t('organizationConsole.teams.disable.title')}</AlertDialogTitle><AlertDialogDescription>{t('organizationConsole.teams.disable.body', { organization: organizationName, team: disabling?.name })}</AlertDialogDescription></AlertDialogHeader>
          {disableMutation.isError ? <p role="alert" className="text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(disableMutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
          <AlertDialogFooter><AlertDialogCancel disabled={disableMutation.isPending}>{t('organizationConsole.actions.cancel')}</AlertDialogCancel><AlertDialogAction disabled={disableMutation.isPending} onClick={(event) => { event.preventDefault(); void disable() }}>{t('organizationConsole.teams.actions.disable')}</AlertDialogAction></AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  )
}

function InvitationsPanel({ organizationId, organizationName, canManage }: { organizationId: number; organizationName: string; canManage: boolean }) {
  const { i18n, t } = useTranslation()
  const query = useOrganizationInvitations(organizationId)
  const teamCatalog = useOrganizationTeamCatalog(organizationId, canManage)
  const resendMutation = useResendOrganizationInvitation(organizationId)
  const revokeMutation = useRevokeOrganizationInvitation(organizationId)
  const [editorOpen, setEditorOpen] = useState(false)
  const [batchOpen, setBatchOpen] = useState(false)
  const [issuedToken, setIssuedToken] = useState<string>()
  const [revoking, setRevoking] = useState<OrganizationInvitationResponse>()
  const invitations = query.data?.invitations ?? []
  const formatTime = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])
  const resend = async (invitation: OrganizationInvitationResponse) => {
    try {
      const result = await resendMutation.mutateAsync(invitation.invitation_key)
      setIssuedToken(result.token)
    } catch {
      // 重发失败时旧邀请状态仍由服务端保持，页面不猜测结果。
    }
  }
  const revoke = async () => {
    if (!revoking) return
    try {
      await revokeMutation.mutateAsync(revoking.invitation_key)
      setRevoking(undefined)
    } catch {
      // 保留确认目标以便用户按同一事实重试。
    }
  }
  return (
    <Card className="overflow-hidden">
      <PanelHeader icon={MailPlus} title={t('organizationConsole.invitations.list.title')} subtitle={t('organizationConsole.invitations.list.subtitle')} count={invitations.length} refreshing={query.isFetching} onRefresh={() => void query.refetch()} actions={canManage ? <div className="flex flex-wrap gap-2"><Button type="button" size="sm" variant="secondary" onClick={() => setBatchOpen(true)}><MoreHorizontal aria-hidden="true" />{t('organizationConsole.invitations.actions.batch')}</Button><Button type="button" size="sm" onClick={() => setEditorOpen(true)}><Send aria-hidden="true" />{t('organizationConsole.invitations.actions.invite')}</Button></div> : undefined} />
      <CardContent className="p-0">
        {query.isPending ? <PanelLoading /> : query.isError ? <PanelState icon={MailPlus} title={t('organizationConsole.errors.listTitle')} body={t('organizationConsole.errors.listBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} /> : invitations.length === 0 ? <PanelState icon={MailPlus} title={t('organizationConsole.invitations.list.emptyTitle')} body={t('organizationConsole.invitations.list.emptyBody')} /> : (
          <div className="divide-y divide-[var(--hairline)]">
            {invitations.map((invitation) => (
              <article key={invitation.invitation_key} className="grid gap-3 px-4 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2"><span className="truncate font-mono text-xs" title={invitation.invitation_key}>{invitation.invitation_key.slice(0, 8)}…{invitation.invitation_key.slice(-8)}</span><Badge>{t(`organizationConsole.roles.${invitation.role}`)}</Badge><Badge className={invitation.status === 'pending' ? 'text-warning' : invitation.status === 'accepted' ? 'text-success' : 'text-muted-foreground'}>{t(`organizationConsole.invitationStatus.${invitation.status}`)}</Badge></div>
                  <p className="mt-1 text-xs text-muted-foreground">{t('organizationConsole.invitations.list.expires', { time: formatTime.format(invitation.expires_at * 1_000) })} · v{invitation.version}</p>
                </div>
                {canManage && invitation.status === 'pending' ? <div className="flex gap-1"><Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.invitations.actions.resend')} disabled={resendMutation.isPending} onClick={() => void resend(invitation)}><RotateCw aria-hidden="true" /><span className="sr-only">{t('organizationConsole.invitations.actions.resend')}</span></Button><Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.invitations.actions.revoke')} disabled={revokeMutation.isPending} onClick={() => setRevoking(invitation)}><Trash2 aria-hidden="true" /><span className="sr-only">{t('organizationConsole.invitations.actions.revoke')}</span></Button></div> : null}
              </article>
            ))}
          </div>
        )}
        {(resendMutation.isError || revokeMutation.isError) ? <p role="alert" className="m-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t('organizationConsole.errors.mutation')}</p> : null}
      </CardContent>
      <OrganizationInvitationEditorSheet organizationId={organizationId} organizationName={organizationName} teams={teamCatalog.data ?? []} open={editorOpen} onOpenChange={setEditorOpen} onIssued={setIssuedToken} />
      <OrganizationBatchInvitationSheet organizationId={organizationId} organizationName={organizationName} teams={teamCatalog.data ?? []} open={batchOpen} onOpenChange={setBatchOpen} />
      <OrganizationInvitationIssuedDialog organizationName={organizationName} token={issuedToken} onClose={() => setIssuedToken(undefined)} />
      <AlertDialog open={revoking !== undefined} onOpenChange={(open) => { if (!open) setRevoking(undefined) }}><AlertDialogContent><AlertDialogHeader><AlertDialogTitle>{t('organizationConsole.invitations.revoke.title')}</AlertDialogTitle><AlertDialogDescription>{t('organizationConsole.invitations.revoke.body', { organization: organizationName })}</AlertDialogDescription></AlertDialogHeader><AlertDialogFooter><AlertDialogCancel disabled={revokeMutation.isPending}>{t('organizationConsole.actions.cancel')}</AlertDialogCancel><AlertDialogAction disabled={revokeMutation.isPending} onClick={(event) => { event.preventDefault(); void revoke() }}>{t('organizationConsole.invitations.actions.revoke')}</AlertDialogAction></AlertDialogFooter></AlertDialogContent></AlertDialog>
    </Card>
  )
}

function SummaryMetric({ icon: Icon, label, value }: { icon: LucideIcon; label: string; value: string }) {
  return <Card><CardContent className="flex items-center gap-3 p-4"><span className="grid size-8 place-items-center rounded-lg bg-surface-2 text-brand"><Icon className="size-4" aria-hidden="true" /></span><div className="min-w-0"><p className="text-xs text-muted-foreground">{label}</p><p className="mt-1 truncate text-sm font-semibold tabular-nums" title={value}>{value}</p></div></CardContent></Card>
}

function PanelHeader({ icon: Icon, title, subtitle, count, refreshing, onRefresh, actions }: { icon: LucideIcon; title: string; subtitle: string; count: number; refreshing: boolean; onRefresh: () => void; actions?: ReactNode }) {
  const { t } = useTranslation()
  return <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45"><div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between"><div className="flex min-w-0 items-start gap-3"><span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><Icon className="size-4" aria-hidden="true" /></span><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><CardTitle className="text-base">{title}</CardTitle><Badge className="bg-surface-2 text-muted-foreground">{count}</Badge></div><p className="mt-1 text-xs leading-5 text-muted-foreground">{subtitle}</p></div></div><div className="flex flex-wrap gap-2"><Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.actions.refresh')} disabled={refreshing} onClick={onRefresh}><RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" /><span className="sr-only">{t('organizationConsole.actions.refresh')}</span></Button>{actions}</div></div></CardHeader>
}

type InfiniteQueryShape = { isPending: boolean; isError: boolean; isFetchingNextPage: boolean; hasNextPage: boolean; fetchNextPage: () => Promise<unknown>; refetch: () => Promise<unknown> }
function PagedContent({ query, items, emptyIcon, emptyTitle, emptyBody, children }: { query: InfiniteQueryShape; items: unknown[]; emptyIcon: LucideIcon; emptyTitle: string; emptyBody: string; children: ReactNode }) {
  const { t } = useTranslation()
  if (query.isPending) return <PanelLoading />
  if (query.isError && items.length === 0) return <PanelState icon={emptyIcon} title={t('organizationConsole.errors.listTitle')} body={t('organizationConsole.errors.listBody')} action={<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}><RefreshCw aria-hidden="true" />{t('organizationConsole.actions.retry')}</Button>} />
  if (items.length === 0) return <PanelState icon={emptyIcon} title={emptyTitle} body={emptyBody} />
  return <>{children}{query.hasNextPage ? <div className="flex justify-center border-t border-[var(--hairline)] p-4"><Button type="button" size="sm" variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t(query.isFetchingNextPage ? 'organizationConsole.actions.loadingMore' : 'organizationConsole.actions.loadMore')}</Button></div> : null}</>
}

function PanelLoading() {
  return <div className="grid gap-3 p-4" role="status">{[0, 1, 2, 3].map((item) => <div key={item} className="grid grid-cols-[2rem_1fr_auto] gap-3"><Skeleton className="size-8 rounded-lg" /><div className="grid gap-2"><Skeleton className="h-4 w-2/3" /><Skeleton className="h-3 w-1/2" /></div><Skeleton className="h-6 w-16" /></div>)}</div>
}

function PanelState({ icon: Icon, title, body, action }: { icon: LucideIcon; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-52 place-items-center p-6 text-center"><div className="max-w-sm"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
