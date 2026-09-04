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

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageLoading, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import { useOrganizationInvitations, useOrganizationMembers } from '@/features/organization-members/organization-member-api'
import type { OrganizationOverviewResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type OrganizationOnboardingPageProps = {
  requestedOrganizationId?: number
}

type OnboardingStepId = 'profile' | 'members' | 'security'
type OnboardingProgress = Record<OnboardingStepId, boolean>

const STEP_IDS: OnboardingStepId[] = ['profile', 'members', 'security']
const PROGRESS_PREFIX = 'anyflows.organization-onboarding'

/** 企业 Owner 初始化向导只消费服务端事实，步骤确认是按企业隔离的本地浏览器进度。 */
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
          action={<Button asChild type="button" size="sm" variant="secondary"><a href="#/console/organization">{t('organizationOnboarding.actions.openOverview')}<ArrowRight aria-hidden="true" /></a></Button>}
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
  const invitationsQuery = useOrganizationInvitations(organizationId)
  const [progress, setProgress] = useState<OnboardingProgress>(() => readProgress(organizationId))
  const [activeStep, setActiveStep] = useState<OnboardingStepId>(() => firstIncomplete(readProgress(organizationId)))
  const sectionRefs = useRef<Partial<Record<OnboardingStepId, HTMLElement | null>>>({})
  const invitations = invitationsQuery.data?.invitations ?? []
  const acceptedInvitations = invitations.filter((invitation) => invitation.status === 'accepted').length
  const pendingInvitations = invitations.filter((invitation) => invitation.status === 'pending').length
  const loadingMembers = membersQuery.isPending || invitationsQuery.isPending
  const loadingError = membersQuery.isError || invitationsQuery.isError

  useEffect(() => {
    writeProgress(organizationId, progress)
  }, [organizationId, progress])

  const refresh = () => {
    void Promise.all([membersQuery.refetch(), invitationsQuery.refetch()])
  }

  const focusStep = (step: OnboardingStepId) => {
    setActiveStep(step)
    const section = sectionRefs.current[step]
    section?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    window.setTimeout(() => section?.focus(), 250)
  }

  const completeStep = (step: OnboardingStepId) => {
    setProgress((current) => ({ ...current, [step]: true }))
    const nextStep = STEP_IDS[STEP_IDS.indexOf(step) + 1]
    if (nextStep) focusStep(nextStep)
  }

  const steps: Array<{ id: OnboardingStepId; icon: LucideIcon; title: string; description: string }> = [
    { id: 'profile', icon: Building2, title: t('organizationOnboarding.steps.profile.title'), description: t('organizationOnboarding.steps.profile.description') },
    { id: 'members', icon: UsersRound, title: t('organizationOnboarding.steps.members.title'), description: t('organizationOnboarding.steps.members.description') },
    { id: 'security', icon: ShieldCheck, title: t('organizationOnboarding.steps.security.title'), description: t('organizationOnboarding.steps.security.description') },
  ]

  return (
    <div className="grid gap-4">
      <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate text-base font-semibold">{overview.name}</h3>
              <Badge className="bg-surface-2 text-muted-foreground">{t(`organizationConsole.roles.${overview.role}`)}</Badge>
              <Badge className={overview.entitlement.status === 'active' ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>{t(`organizationConsole.entitlement.${overview.entitlement.status}`)}</Badge>
            </div>
            <p className="mt-1 truncate text-xs text-muted-foreground">{overview.slug} · #{overview.id}</p>
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <CheckCircle2 className="size-4 text-success" aria-hidden="true" />
            {t('organizationOnboarding.localProgress', { completed: STEP_IDS.filter((step) => progress[step]).length, total: STEP_IDS.length })}
          </div>
        </div>
        <p className="mt-3 border-t border-[var(--hairline)] pt-3 text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.localProgressHint')}</p>
      </section>

      <nav className="sticky top-2 z-10 -mx-1 overflow-x-auto rounded-xl border border-[var(--hairline)] bg-background/95 p-1 backdrop-blur" aria-label={t('organizationOnboarding.stepsLabel')}>
        <div className="flex min-w-max gap-1">
          {steps.map((step, index) => {
            const completed = progress[step.id]
            return (
              <button
                key={step.id}
                type="button"
                className={cn('flex items-center gap-2 rounded-lg px-3 py-2 text-left text-xs transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60', activeStep === step.id ? 'bg-surface-2 text-foreground' : 'text-muted-foreground hover:bg-surface-2/70')}
                aria-current={activeStep === step.id ? 'step' : undefined}
                onClick={() => focusStep(step.id)}
              >
                <span className="grid size-5 place-items-center rounded-full border border-[var(--hairline)] text-[0.6875rem] tabular-nums">{completed ? <CheckCircle2 className="size-3.5 text-success" aria-hidden="true" /> : index + 1}</span>
                <span>{step.title}</span>
              </button>
            )
          })}
        </div>
      </nav>

      <div className="grid gap-4">
        <OnboardingSection
          id="profile"
          icon={Building2}
          title={steps[0].title}
          description={steps[0].description}
          completed={progress.profile}
          sectionRef={(section) => { sectionRefs.current.profile = section }}
        >
          <div className="grid gap-0 rounded-lg border border-[var(--hairline)] sm:grid-cols-2">
            <ReadOnlyFact label={t('organizationOnboarding.profile.name')} value={overview.name} />
            <ReadOnlyFact label={t('organizationOnboarding.profile.slug')} value={overview.slug} mono />
            <ReadOnlyFact label={t('organizationOnboarding.profile.organizationId')} value={`#${overview.id}`} mono />
            <ReadOnlyFact label={t('organizationOnboarding.profile.role')} value={t(`organizationConsole.roles.${overview.role}`)} />
          </div>
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.profile.serverFact')}</p>
          <SectionActions completed={progress.profile} onComplete={() => completeStep('profile')} href="#/console/organization-settings" linkLabel={t('organizationOnboarding.actions.openSettings')} />
        </OnboardingSection>

        <OnboardingSection
          id="members"
          icon={UsersRound}
          title={steps[1].title}
          description={steps[1].description}
          completed={progress.members}
          sectionRef={(section) => { sectionRefs.current.members = section }}
        >
          {loadingMembers ? <FactLoading /> : loadingError ? <FactError onRetry={refresh} /> : (
            <div className="grid gap-3 sm:grid-cols-3">
              <FactMetric icon={UsersRound} label={t('organizationOnboarding.members.active')} value={String(overview.entitlement.active_seats)} hint={t('organizationOnboarding.members.activeHint', { limit: overview.entitlement.seat_limit })} />
              <FactMetric icon={MailPlus} label={t('organizationOnboarding.members.pending')} value={String(pendingInvitations)} hint={t('organizationOnboarding.members.pendingHint')} />
              <FactMetric icon={CheckCircle2} label={t('organizationOnboarding.members.accepted')} value={String(acceptedInvitations)} hint={t('organizationOnboarding.members.acceptedHint')} />
            </div>
          )}
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.members.serverFact')}</p>
          <SectionActions completed={progress.members} onComplete={() => completeStep('members')} href="#/console/organization-members" linkLabel={t('organizationOnboarding.actions.openMembers')} />
        </OnboardingSection>

        <OnboardingSection
          id="security"
          icon={ShieldCheck}
          title={steps[2].title}
          description={steps[2].description}
          completed={progress.security}
          sectionRef={(section) => { sectionRefs.current.security = section }}
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <FactMetric icon={ShieldCheck} label={t('organizationOnboarding.security.sso')} value={overview.entitlement.sso_enabled ? t('organizationOnboarding.security.enabled') : t('organizationOnboarding.security.disabled')} hint={t('organizationOnboarding.security.ssoHint')} />
            <FactMetric icon={ClipboardCheck} label={t('organizationOnboarding.security.role')} value={t(`organizationConsole.roles.${overview.role}`)} hint={t('organizationOnboarding.security.roleHint')} />
          </div>
          <p className="text-xs leading-5 text-muted-foreground">{t('organizationOnboarding.security.serverFact')}</p>
          <SectionActions completed={progress.security} onComplete={() => completeStep('security')} href="#/console/organization-settings" linkLabel={t('organizationOnboarding.actions.openSettings')} />
        </OnboardingSection>
      </div>
    </div>
  )
}

function OnboardingSection({ id, icon: Icon, title, description, completed, sectionRef, children }: { id: OnboardingStepId; icon: LucideIcon; title: string; description: string; completed: boolean; sectionRef: (section: HTMLElement | null) => void; children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <section id={`organization-onboarding-${id}`} ref={sectionRef} tabIndex={-1} className="scroll-mt-20">
      <Card className="overflow-hidden">
        <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
          <div className="flex items-start gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><Icon className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2"><CardTitle className="text-base">{title}</CardTitle>{completed ? <Badge className="bg-success/10 text-success"><CheckCircle2 className="mr-1 size-3" aria-hidden="true" />{t('organizationOnboarding.completed')}</Badge> : null}</div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p>
            </div>
          </div>
        </CardHeader>
        <CardContent className="grid gap-4 p-4">{children}</CardContent>
      </Card>
    </section>
  )
}

function SectionActions({ completed, onComplete, href, linkLabel }: { completed: boolean; onComplete: () => void; href: string; linkLabel: string }) {
  const { t } = useTranslation()
  return <div className="flex flex-wrap items-center gap-2"><Button asChild type="button" size="sm" variant="secondary"><a href={href}>{linkLabel}<ArrowRight aria-hidden="true" /></a></Button><Button type="button" size="sm" variant={completed ? 'secondary' : 'default'} onClick={onComplete}>{completed ? <CheckCircle2 aria-hidden="true" /> : <ClipboardCheck aria-hidden="true" />}{t(completed ? 'organizationOnboarding.actions.confirmed' : 'organizationOnboarding.actions.confirm')}</Button></div>
}

function ReadOnlyFact({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <div className="min-w-0 border-b border-[var(--hairline)] px-3 py-2.5 last:border-b-0 sm:border-r sm:last:border-r-0"><p className="text-[0.6875rem] text-muted-foreground">{label}</p><p className={cn('mt-1 truncate text-sm font-medium', mono && 'font-mono')} title={value}>{value}</p></div>
}

function FactMetric({ icon: Icon, label, value, hint }: { icon: LucideIcon; label: string; value: string; hint: string }) {
  return <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-3"><div className="flex items-center justify-between gap-2"><p className="text-[0.6875rem] text-muted-foreground">{label}</p><Icon className="size-4 text-brand" aria-hidden="true" /></div><p className="mt-2 text-lg font-semibold tabular-nums">{value}</p><p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p></div>
}

function FactLoading() {
  const { t } = useTranslation()
  return <div className="flex items-center gap-2 rounded-lg bg-surface-2/55 px-3 py-3 text-xs text-muted-foreground" role="status"><RefreshCw className="size-3.5 animate-spin" aria-hidden="true" />{t('organizationOnboarding.members.loading')}</div>
}

function FactError({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()
  return <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-3 text-xs text-destructive" role="alert"><span>{t('organizationOnboarding.members.loadError')}</span><Button type="button" size="sm" variant="secondary" onClick={onRetry}><RefreshCw aria-hidden="true" />{t('organizationOnboarding.actions.retry')}</Button></div>
}

function readProgress(organizationId: number): OnboardingProgress {
  try {
    const raw = window.localStorage.getItem(`${PROGRESS_PREFIX}.${organizationId}`)
    if (!raw) return emptyProgress()
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null) return emptyProgress()
    const record = parsed as Record<string, unknown>
    return {
      profile: record.profile === true,
      members: record.members === true,
      security: record.security === true,
    }
  } catch {
    return emptyProgress()
  }
}

function writeProgress(organizationId: number, progress: OnboardingProgress) {
  try {
    window.localStorage.setItem(`${PROGRESS_PREFIX}.${organizationId}`, JSON.stringify(progress))
  } catch {
    // 浏览器拒绝持久化时仍保留当前页面内的确认状态。
  }
}

function emptyProgress(): OnboardingProgress {
  return { profile: false, members: false, security: false }
}

function firstIncomplete(progress: OnboardingProgress): OnboardingStepId {
  return STEP_IDS.find((step) => !progress[step]) ?? 'profile'
}
