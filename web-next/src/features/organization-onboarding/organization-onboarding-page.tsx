import { Button, Card, CardBody, CardHeader, Chip } from '@heroui/react'
import {
  ArrowRight,
  Building2,
  CheckCircle2,
  ClipboardCheck,
  MailPlus,
  RefreshCw,
  ShieldCheck,
  UsersRound,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageLoading, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import { OrganizationProfileEditor } from '@/features/organization-console/organization-profile-editor'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import { OrganizationInvitationEditorSheet } from '@/features/organization-members/organization-invitation-editor-sheet'
import { OrganizationInvitationIssuedDialog } from '@/features/organization-members/organization-invitation-issued-dialog'
import { useOrganizationInvitations, useOrganizationMembers, useOrganizationTeams } from '@/features/organization-members/organization-member-api'
import { useOrganizationSsoPolicy, useOrganizationSsoProviders } from '@/features/organization-sso/organization-sso-api'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type OrganizationOnboardingPageProps = {
  requestedOrganizationId?: number
}

type OnboardingStepId = 'profile' | 'members' | 'security'

const STEP_IDS: OnboardingStepId[] = ['profile', 'members', 'security']

/** 企业 Owner 初始化向导只使用服务端事实计算进度，并在步骤内提供真实操作。 */
export function OrganizationOnboardingPage({ requestedOrganizationId }: OrganizationOnboardingPageProps) {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  const workspace = useOrganizationWorkspace()
  const directoryReady = workspace.organizationsQuery.isSuccess
  const requestedOrganization = requestedOrganizationId === undefined
    ? undefined
    : workspace.organizations.find((organization) => organization.id === requestedOrganizationId)
  const requestedUnavailable = directoryReady
    && requestedOrganizationId !== undefined
    && workspace.organizations.length > 0
    && requestedOrganization === undefined

  useEffect(() => {
    if (!requestedOrganization || workspace.selectedId === requestedOrganization.id || workspace.switching) return
    void workspace.selectOrganization(requestedOrganization.id)
  }, [requestedOrganization, workspace])

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader
        icon={ClipboardCheck}
        title={t('organizationOnboarding.title')}
        subtitle={t('organizationOnboarding.subtitle')}
      />
      {requestedUnavailable ? (
        <OrganizationPageState
          icon={Building2}
          title={t('organizationOnboarding.errors.organizationUnavailableTitle')}
          body={t('organizationOnboarding.errors.organizationUnavailableBody')}
          action={<Button as="a" href="/console/organization" size="sm" type="button" variant="bordered">{t('organizationOnboarding.actions.openOverview')}<ArrowRight className="size-3.5" aria-hidden="true" /></Button>}
        />
      ) : requestedOrganizationId !== undefined && workspace.selectedId !== requestedOrganizationId ? (
        <OrganizationPageLoading />
      ) : (
        <OrganizationPageBoundary state={state}>
          {(organizationId, overview) => <OrganizationOnboardingWorkspace key={organizationId} organizationId={organizationId} overview={overview} />}
        </OrganizationPageBoundary>
      )}
    </div>
  )
}

function OrganizationOnboardingWorkspace({ organizationId, overview }: { organizationId: number; overview: OrganizationOverviewResponse }) {
  const { t } = useTranslation()
  const membersQuery = useOrganizationMembers(organizationId)
  const canInvite = overview.permissions.includes('organization.members.manage')
  const canReadSso = overview.permissions.includes('organization.sso.read')
  const invitationsQuery = useOrganizationInvitations(organizationId, canInvite)
  const teamsQuery = useOrganizationTeams(organizationId, canInvite)
  const providersQuery = useOrganizationSsoProviders(organizationId, overview.entitlement.sso_enabled && canReadSso)
  const policyQuery = useOrganizationSsoPolicy(organizationId, overview.entitlement.sso_enabled && canReadSso)
  const [activeStep, setActiveStep] = useState<OnboardingStepId>('members')
  const [invitationOpen, setInvitationOpen] = useState(false)
  const [issuedToken, setIssuedToken] = useState<string>()
  const sectionRefs = useRef<Partial<Record<OnboardingStepId, HTMLElement | null>>>({})
  const invitations = invitationsQuery.data?.invitations ?? []
  const acceptedInvitations = invitations.filter((invitation) => invitation.status === 'accepted').length
  const pendingInvitations = invitations.filter((invitation) => invitation.status === 'pending').length
  const teams = teamsQuery.data?.pages.flatMap((page) => page.teams) ?? []
  const profileReady = overview.name.trim().length > 0 && overview.slug.trim().length > 0
  const membersReady = overview.entitlement.active_seats > 1 || pendingInvitations > 0
  const securityReady = !overview.entitlement.sso_enabled
    || (providersQuery.data?.some((provider) => provider.status === 'active') === true && policyQuery.data !== undefined)
  const progress: Record<OnboardingStepId, boolean> = { profile: profileReady, members: membersReady, security: securityReady }
  const completedCount = STEP_IDS.filter((step) => progress[step]).length
  const loadingMembers = membersQuery.isPending || (canInvite && invitationsQuery.isPending)
  const loadingError = membersQuery.isError || (canInvite && invitationsQuery.isError)

  const refreshMembers = () => {
    if (canInvite) {
      void Promise.all([membersQuery.refetch(), invitationsQuery.refetch()])
    } else {
      void membersQuery.refetch()
    }
  }

  const focusStep = (step: OnboardingStepId) => {
    setActiveStep(step)
    const section = sectionRefs.current[step]
    section?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    window.setTimeout(() => section?.focus(), 250)
  }

  const steps: Array<{ id: OnboardingStepId; icon: LucideIcon; title: string; description: string }> = [
    { id: 'profile', icon: Building2, title: t('organizationOnboarding.steps.profile.title'), description: t('organizationOnboarding.steps.profile.description') },
    { id: 'members', icon: UsersRound, title: t('organizationOnboarding.steps.members.title'), description: t('organizationOnboarding.steps.members.description') },
    { id: 'security', icon: ShieldCheck, title: t('organizationOnboarding.steps.security.title'), description: t('organizationOnboarding.steps.security.description') },
  ]

  return (
    <div className="grid gap-4">
      <section className="rounded-lg border border-[var(--hairline)] bg-surface-1/45 p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate text-base font-semibold">{overview.name}</h3>
              <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">{t(`organizationConsole.roles.${overview.role}`)}</Chip>
              <Chip className={overview.entitlement.status === 'active' ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'} size="sm" variant="flat">{t(`organizationConsole.entitlement.${overview.entitlement.status}`)}</Chip>
            </div>
            <p className="mt-1 truncate text-xs text-muted-foreground">{overview.slug} · #{overview.id}</p>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <CheckCircle2 className="size-4 text-success" aria-hidden="true" />
            {t('organizationOnboarding.serverProgress', { completed: completedCount, total: STEP_IDS.length })}
          </div>
        </div>
        <p className="mt-3 border-t border-[var(--hairline)] pt-3 text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.serverProgressHint')}</p>
      </section>

      <nav className="sticky top-2 z-10 -mx-1 overflow-x-auto rounded-lg border border-[var(--hairline)] bg-background/95 p-1 backdrop-blur" aria-label={t('organizationOnboarding.stepsLabel')}>
        <div className="flex min-w-max gap-1">
          {steps.map((step, index) => (
            <button
              key={step.id}
              type="button"
              className={cn('flex items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60', activeStep === step.id ? 'bg-surface-2 text-foreground' : 'text-muted-foreground hover:bg-surface-2/70')}
              aria-current={activeStep === step.id ? 'step' : undefined}
              onClick={() => focusStep(step.id)}
            >
              <span className="grid size-5 place-items-center rounded-full border border-[var(--hairline)] text-[0.6875rem] tabular-nums">{progress[step.id] ? <CheckCircle2 className="size-3.5 text-success" aria-hidden="true" /> : index + 1}</span>
              <span>{step.title}</span>
            </button>
          ))}
        </div>
      </nav>

      <div className="grid gap-4">
        <OnboardingSection id="profile" icon={Building2} title={steps[0].title} description={steps[0].description} completed={profileReady} sectionRef={(section) => { sectionRefs.current.profile = section }}>
          <OrganizationProfileEditor organizationId={organizationId} overview={overview} />
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.profile.serverFact')}</p>
        </OnboardingSection>

        <OnboardingSection id="members" icon={UsersRound} title={steps[1].title} description={steps[1].description} completed={membersReady} sectionRef={(section) => { sectionRefs.current.members = section }}>
          {loadingMembers ? <FactLoading /> : loadingError ? <FactError onRetry={refreshMembers} /> : (
            <div className="grid gap-3 sm:grid-cols-3">
              <FactMetric icon={UsersRound} label={t('organizationOnboarding.members.active')} value={String(overview.entitlement.active_seats)} hint={t('organizationOnboarding.members.activeHint', { limit: overview.entitlement.seat_limit })} />
              <FactMetric icon={MailPlus} label={t('organizationOnboarding.members.pending')} value={String(pendingInvitations)} hint={t('organizationOnboarding.members.pendingHint')} />
              <FactMetric icon={CheckCircle2} label={t('organizationOnboarding.members.accepted')} value={String(acceptedInvitations)} hint={t('organizationOnboarding.members.acceptedHint')} />
            </div>
          )}
          <p className="text-xs leading-5 text-muted-foreground">{t(membersReady ? 'organizationOnboarding.members.completeHint' : 'organizationOnboarding.members.incompleteHint')}</p>
          <div className="flex flex-wrap items-center gap-2">
            {canInvite ? <Button type="button" size="sm" color="primary" onClick={() => setInvitationOpen(true)}><MailPlus className="size-3.5" aria-hidden="true" />{t('organizationOnboarding.actions.inviteMember')}</Button> : null}
            <Button as="a" href="/console/organization-members" size="sm" type="button" variant="bordered">{t('organizationOnboarding.actions.openMembers')}<ArrowRight className="size-3.5" aria-hidden="true" /></Button>
          </div>
        </OnboardingSection>

        <OnboardingSection id="security" icon={ShieldCheck} title={steps[2].title} description={steps[2].description} completed={securityReady} sectionRef={(section) => { sectionRefs.current.security = section }}>
          <div className="grid gap-3 sm:grid-cols-2">
            <FactMetric icon={ShieldCheck} label={t('organizationOnboarding.security.sso')} value={overview.entitlement.sso_enabled ? t('organizationOnboarding.security.enabled') : t('organizationOnboarding.security.notIncluded')} hint={overview.entitlement.sso_enabled ? t('organizationOnboarding.security.ssoHint') : t('organizationOnboarding.security.basicReady')} />
            <FactMetric icon={ClipboardCheck} label={t('organizationOnboarding.security.role')} value={t(`organizationConsole.roles.${overview.role}`)} hint={t('organizationOnboarding.security.roleHint')} />
          </div>
          {overview.entitlement.sso_enabled && canReadSso ? (
            providersQuery.isPending || policyQuery.isPending ? <FactLoading /> : providersQuery.isError || policyQuery.isError ? <FactError onRetry={() => void Promise.all([providersQuery.refetch(), policyQuery.refetch()])} /> : (
              <p className="text-xs leading-5 text-muted-foreground">{t(securityReady ? 'organizationOnboarding.security.configured' : 'organizationOnboarding.security.needsConfiguration')}</p>
            )
          ) : <p className="text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.security.serverFact')}</p>}
          <Button as="a" href="/console/organization-settings" className="justify-self-start" size="sm" type="button" variant="bordered">{t('organizationOnboarding.actions.openSettings')}<ArrowRight className="size-3.5" aria-hidden="true" /></Button>
        </OnboardingSection>
      </div>

      <OrganizationInvitationEditorSheet organizationId={organizationId} organizationName={overview.name} teams={teams} open={invitationOpen} onOpenChange={setInvitationOpen} onIssued={setIssuedToken} />
      <OrganizationInvitationIssuedDialog organizationName={overview.name} token={issuedToken} onClose={() => setIssuedToken(undefined)} />
    </div>
  )
}

function OnboardingSection({ id, icon: Icon, title, description, completed, sectionRef, children }: { id: OnboardingStepId; icon: LucideIcon; title: string; description: string; completed: boolean; sectionRef: (section: HTMLElement | null) => void; children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <section id={`organization-onboarding-${id}`} ref={sectionRef} tabIndex={-1} className="scroll-mt-20">
      <Card className="overflow-hidden border border-[var(--hairline)]" shadow="none">
        <CardHeader className="flex-col items-stretch gap-1.5 border-b border-[var(--hairline)] bg-surface-1/45 p-5">
          <div className="flex items-start gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><Icon className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2"><h3 className="text-base font-semibold">{title}</h3><Chip className={completed ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'} size="sm" startContent={completed ? <CheckCircle2 className="size-3" aria-hidden="true" /> : undefined} variant="flat">{t(completed ? 'organizationOnboarding.completed' : 'organizationOnboarding.pending')}</Chip></div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>
            </div>
          </div>
        </CardHeader>
        <CardBody className="grid gap-4 p-4">{children}</CardBody>
      </Card>
    </section>
  )
}

function FactMetric({ icon: Icon, label, value, hint }: { icon: LucideIcon; label: string; value: string; hint: string }) {
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-3"><div className="flex items-center justify-between gap-2"><p className="text-[0.6875rem] text-muted-foreground">{label}</p><Icon className="size-4 text-brand" aria-hidden="true" /></div><p className="mt-2 text-lg font-semibold tabular-nums">{value}</p><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p></div>
}

function FactLoading() {
  const { t } = useTranslation()
  return <div className="flex items-center gap-2 rounded-lg bg-surface-2/55 px-3 py-3 text-xs text-muted-foreground" role="status"><RefreshCw className="size-3.5 animate-spin" aria-hidden="true" />{t('organizationOnboarding.loading')}</div>
}

function FactError({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()
  return <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-3 text-xs text-destructive" role="alert"><span>{t('organizationOnboarding.loadError')}</span><Button type="button" size="sm" variant="bordered" onClick={onRetry}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationOnboarding.actions.retry')}</Button></div>
}
