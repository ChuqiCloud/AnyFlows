import { AccountVerificationPage } from '@/features/account-verification/account-verification-page'
import { VerificationSettingsPanel } from '@/features/account-verification/verification-settings-panel'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { routeFromHash, type AppRoute, type AppView } from '@/app-route'
import { AppShell } from '@/components/layout/app-shell'
import { AuthenticationSettingsPage } from '@/features/authentication-settings/authentication-settings-page'
import { ForgotPasswordPage } from '@/features/auth/forgot-password-page'
import { LoginPage, type LoginNotice } from '@/features/auth/login-page'
import { OAuthCallbackPage } from '@/features/auth/oauth-callback-page'
import { ResetPasswordPage } from '@/features/auth/reset-password-page'
import { SessionBoundary } from '@/features/auth/session-boundary'
import { endManagementSession } from '@/features/auth/session-query'
import { ApiKeyPage } from '@/features/api-keys/api-key-page'
import { BillingSettingsPage } from '@/features/billing-settings/billing-settings-page'
import { AnnouncementPage } from '@/features/announcements/announcement-page'
import { ApiExplorerPage } from '@/features/api-explorer/api-explorer-page'
import { ChannelPage } from '@/features/channels/channel-page'
import { CredentialPage } from '@/features/credentials/credential-page'
import { CredentialProxyPage } from '@/features/credential-proxies/credential-proxy-page'
import { DashboardPage } from '@/features/dashboard/dashboard-page'
import { DebugTracePage } from '@/features/debug-traces/debug-trace-page'
import { EmailSettingsPage } from '@/features/email-settings/email-settings-page'
import { GroupPage } from '@/features/groups/group-page'
import { InvitationPage } from '@/features/invitations/invitation-page'
import { LandingPage } from '@/features/landing/landing-page'
import { ModelPage } from '@/features/models/model-page'
import { ModelManagementPage } from '@/features/model-management/model-management-page'
import { ModelProviderCatalogPanel } from '@/features/model-management/model-provider-catalog-panel'
import { NetworkSettingsPage } from '@/features/network-settings/network-settings-page'
import { OrganizationActivityPage } from '@/features/organization-activity/organization-activity-page'
import { OrganizationApiKeyPage } from '@/features/organization-api-keys/organization-api-key-page'
import { OrganizationBudgetPage } from '@/features/organization-budgets/organization-budget-page'
import { OrganizationContractPricePage } from '@/features/organization-contract-prices/organization-contract-price-page'
import { OrganizationCreditPage } from '@/features/organization-credit/organization-credit-page'
import { OrganizationApprovalPage } from '@/features/organization-approvals/organization-approval-page'
import { OrganizationMemberPage } from '@/features/organization-members/organization-member-page'
import { OrganizationDepartmentPage } from '@/features/organization-departments/organization-department-page'
import { OrganizationOverviewPage } from '@/features/organization-console/organization-overview-page'
import { OrganizationOnboardingPage } from '@/features/organization-onboarding/organization-onboarding-page'
import { OrganizationPlanManagementPage } from '@/features/organization-console/organization-plan-management-page'
import { OrganizationProvisioningPage } from '@/features/organization-provisioning/organization-provisioning-page'
import { OrganizationVerificationPage } from '@/features/organization-verification/organization-verification-page'
import { OrganizationSettingsPage } from '@/features/organization-console/organization-settings-page'
import { OrganizationServiceAccountPage } from '@/features/organization-service-accounts/organization-service-account-page'
import { OrganizationWalletPage } from '@/features/organization-wallet/organization-wallet-page'
import { OrganizationWorkspaceProvider } from '@/features/organization-console/organization-workspace-context'
import { PaymentSettingsPage } from '@/features/payment-settings/payment-settings-page'
import { PlatformOrganizationPage } from '@/features/platform-organizations/platform-organization-page'
import { PublicModelPage } from '@/features/models/public-model-page'
import { PlaygroundPage } from '@/features/playground/playground-page'
import { PublicPlaygroundSharePage } from '@/features/playground/public-playground-share-page'
import { ProfilePage } from '@/features/profile/profile-page'
import { RegistrationPage } from '@/features/registration/registration-page'
import { RedemptionPage } from '@/features/redemptions/redemption-page'
import { RefundPage } from '@/features/refunds/refund-page'
import { RoutePage } from '@/features/routes/route-page'
import { SiteSettingsPage } from '@/features/site-settings/site-settings-page'
import { MySubscriptionsPage } from '@/features/subscriptions/my-subscriptions-page'
import { SubscriptionManagementPage } from '@/features/subscriptions/subscription-management-page'
import { TokenPage } from '@/features/tokens/token-page'
import { UsageLogPage } from '@/features/usage-logs/usage-log-page'
import { UserPage } from '@/features/users/user-page'
import { VideoTaskPage } from '@/features/video-tasks/video-task-page'
import { WalletPage } from '@/features/wallet/wallet-page'
import { SetupGateLoading, SetupGateUnavailable } from '@/features/setup/setup-gate-state'
import { SetupPage } from '@/features/setup/setup-page'
import { useInitialSetupStatus } from '@/features/setup/setup-query'

// 临时的 hash 视图切换：TanStack Router 接入前，先让公开页与控制台都可访达。
// 路由落地后此处整体由文件路由替换。
function useHashRoute(): [AppRoute, (hash: string) => void] {
  const [hash, setHash] = useState(() => window.location.hash)

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash)

    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  const navigate = useCallback((nextHash: string) => {
    // 同步更新 React 路由状态，避免 setup 缓存切换与原生 hashchange 之间出现竞态。
    setHash(nextHash)
    window.location.hash = nextHash
  }, [])

  return [routeFromHash(hash), navigate]
}

type ManagementEntryProps = {
  view: Exclude<AppView, 'landing' | 'public-models' | 'register' | 'share' | 'forgot-password' | 'reset-password' | 'oauth-callback'>
  credentialChannelId?: number
  profileSection?: AppRoute['profileSection']
  activityTab?: AppRoute['activityTab']
  organizationId?: AppRoute['organizationId']
  loginNotice?: LoginNotice
  onAuthenticated: () => void
  onLoginRequired: (reason: 'required' | 'sessionExpired' | 'setupComplete') => void
  onUserHomeRequired: () => void
  onPasswordChanged: () => void
  onLogout: () => void
}

/** 在登录或控制台挂载前完成首次安装状态判定。 */
function ManagementEntry({
  view,
  credentialChannelId,
  profileSection,
  activityTab,
  organizationId,
  loginNotice,
  onAuthenticated,
  onLoginRequired,
  onUserHomeRequired,
  onPasswordChanged,
  onLogout,
}: ManagementEntryProps) {
  const { t } = useTranslation()
  const setupQuery = useInitialSetupStatus()

  useEffect(() => {
    if (setupQuery.data?.setup_required && view !== 'setup') {
      window.location.hash = '#/setup'
    } else if (setupQuery.data && !setupQuery.data.setup_required && view === 'setup') {
      onLoginRequired('required')
    }
  }, [onLoginRequired, setupQuery.data, view])

  if (setupQuery.isPending) {
    return <SetupGateLoading />
  }
  if (setupQuery.isError) {
    return <SetupGateUnavailable onRetry={() => void setupQuery.refetch()} />
  }
  if (setupQuery.data.setup_required) {
    return (
      <SetupPage
        onAuthenticated={onAuthenticated}
        onConflict={() => onLoginRequired('setupComplete')}
      />
    )
  }
  if (view === 'setup') {
    return <SetupGateLoading />
  }
  if (view === 'login') {
    return <LoginPage notice={loginNotice} onAuthenticated={onAuthenticated} />
  }

  const titleKey = view === 'channels'
    ? 'channels.title'
    : view === 'announcements'
      ? 'announcements.title'
    : view === 'credentials'
      ? 'credentials.title'
    : view === 'credential-proxies'
      ? 'credentialProxies.title'
    : view === 'site-settings'
      ? 'siteSettings.title'
      : view === 'authentication-settings'
        ? 'authenticationSettings.title'
          : view === 'redemption-codes'
            ? 'redemptions.title'
          : view === 'refunds'
            ? 'refunds.title'
          : view === 'subscription-management'
            ? 'subscriptions.managementTitle'
          : view === 'subscriptions'
            ? 'subscriptions.title'
        : view === 'routes'
          ? 'routes.title'
        : view === 'email-settings'
          ? 'emailSettings.title'
          : view === 'network-settings'
            ? 'networkSettings.title'
          : view === 'payment-settings'
            ? 'paymentSettings.title'
          : view === 'billing-settings'
            ? 'billingSettings.title'
          : view === 'group-settings'
            ? 'groups.title'
              : view === 'model-providers'
                ? 'modelManagement.title'
              : view === 'account-verification-settings'
                ? 'verificationCenter.settingsTitle'
              : view === 'model-settings'
              ? 'modelManagement.title'
              : view === 'users'
                ? 'users.title'
            : view === 'api-keys'
              ? 'apiKeys.title'
              : view === 'organization-api-keys'
                ? 'organizationApiKeys.title'
                : view === 'organization-activity'
                  ? 'organizationActivity.title'
                : view === 'organization-budgets'
                  ? 'organizationBudgets.title'
                : view === 'organization-contract-prices'
                  ? 'organizationContractPrices.title'
                : view === 'organization-credit'
                  ? 'organizationCredit.title'
                : view === 'organization-approvals'
                  ? 'organizationApprovals.title'
                : view === 'organization-members'
                  ? 'organizationConsole.members.title'
                : view === 'organization-departments'
                  ? 'organizationDepartments.title'
                : view === 'organization-overview'
                  ? 'organizationConsole.overview.title'
                : view === 'organization-onboarding'
                  ? 'organizationOnboarding.title'
                : view === 'organization-plans'
                  ? 'organizationPlans.title'
                : view === 'platform-organizations'
                  ? 'platformOrganizations.title'
                : view === 'organization-provisioning'
                  ? 'organizationProvisioning.title'
                : view === 'organization-verification'
                  ? 'organizationVerification.title'
                : view === 'organization-settings'
                  ? 'organizationConsole.settings.title'
                : view === 'organization-service-accounts'
                  ? 'organizationServiceAccounts.title'
                : view === 'organization-wallet'
                  ? 'organizationConsole.wallet.title'
              : view === 'wallet'
                ? 'wallet.title'
              : view === 'invitations'
                ? 'invitations.title'
              : view === 'models'
                ? 'models.title'
                : view === 'playground'
                  ? 'playground.title'
                  : view === 'video-tasks'
                    ? 'videoTasks.title'
                  : view === 'account-verification'
                    ? 'verificationCenter.title'
                  : view === 'profile'
                    ? 'profile.title'
                    : view === 'tokens'
                      ? 'tokens.title'
                      : view === 'usage-logs'
                        ? 'usageLogs.title'
                        : view === 'debug-traces'
                          ? 'debugTraces.title'
                        : 'dashboard.title'
  const content = (admin: boolean) => view === 'channels'
    ? <ChannelPage />
    : view === 'announcements'
      ? <AnnouncementPage />
    : view === 'credentials'
      ? <CredentialPage initialChannelId={credentialChannelId} />
    : view === 'credential-proxies'
      ? <CredentialProxyPage />
    : view === 'site-settings'
      ? <SiteSettingsPage />
      : view === 'authentication-settings'
        ? <AuthenticationSettingsPage />
        : view === 'redemption-codes'
          ? <RedemptionPage />
        : view === 'refunds'
          ? <RefundPage />
        : view === 'subscription-management'
          ? <SubscriptionManagementPage />
        : view === 'subscriptions'
          ? <MySubscriptionsPage />
        : view === 'routes'
          ? <RoutePage />
        : view === 'email-settings'
          ? <EmailSettingsPage />
          : view === 'network-settings'
            ? <NetworkSettingsPage />
          : view === 'payment-settings'
            ? <PaymentSettingsPage />
          : view === 'billing-settings'
            ? <BillingSettingsPage />
          : view === 'group-settings'
            ? <GroupPage />
            : view === 'model-providers'
              ? <ModelProviderCatalogPanel />
            : view === 'account-verification-settings'
              ? <VerificationSettingsPanel />
            : view === 'model-settings'
              ? <ModelManagementPage />
              : view === 'users'
                ? <UserPage />
            : view === 'api-keys'
              ? <ApiKeyPage />
            : view === 'organization-api-keys'
                ? <OrganizationApiKeyPage />
                : view === 'organization-activity'
                  ? <OrganizationActivityPage initialTab={activityTab} />
                : view === 'organization-budgets'
                  ? <OrganizationBudgetPage />
                : view === 'organization-contract-prices'
                  ? <OrganizationContractPricePage />
                : view === 'organization-credit'
                  ? <OrganizationCreditPage />
                : view === 'organization-approvals'
                  ? <OrganizationApprovalPage />
                : view === 'organization-members'
                  ? <OrganizationMemberPage />
                : view === 'organization-departments'
                  ? <OrganizationDepartmentPage />
                : view === 'organization-overview'
                  ? <OrganizationOverviewPage />
                : view === 'organization-onboarding'
                  ? <OrganizationOnboardingPage requestedOrganizationId={organizationId} />
                : view === 'organization-plans'
                  ? <OrganizationPlanManagementPage />
                : view === 'platform-organizations'
                  ? <PlatformOrganizationPage />
                : view === 'organization-provisioning'
                  ? <OrganizationProvisioningPage admin={admin} />
                : view === 'organization-verification'
                  ? <OrganizationVerificationPage admin={admin} />
                : view === 'organization-settings'
                  ? <OrganizationSettingsPage />
                : view === 'organization-service-accounts'
                  ? <OrganizationServiceAccountPage />
                : view === 'organization-wallet'
                  ? <OrganizationWalletPage />
              : view === 'wallet'
                ? <WalletPage />
              : view === 'invitations'
                ? <InvitationPage />
              : view === 'models'
                ? <ModelPage authenticated admin={admin} />
                : view === 'playground'
                  ? <PlaygroundPage />
                  : view === 'video-tasks'
                    ? <VideoTaskPage />
                  : view === 'account-verification'
                    ? <AccountVerificationPage />
                  : view === 'profile'
                    ? <ProfilePage section={profileSection} onPasswordChanged={onPasswordChanged} />
                    : view === 'tokens'
                      ? <TokenPage />
                      : view === 'usage-logs'
                        ? <UsageLogPage admin={admin} />
                        : view === 'debug-traces'
                          ? <DebugTracePage />
                        : <DashboardPage />

  const requireAdmin = view !== 'api-keys'
    && view !== 'organization-api-keys'
    && view !== 'organization-activity'
    && view !== 'organization-budgets'
    && view !== 'organization-contract-prices'
    && view !== 'organization-credit'
    && view !== 'organization-approvals'
    && view !== 'organization-members'
    && view !== 'organization-departments'
    && view !== 'organization-overview'
    && view !== 'organization-onboarding'
    && view !== 'organization-plans'
    && view !== 'organization-provisioning'
    && view !== 'organization-verification'
    && view !== 'organization-settings'
    && view !== 'organization-service-accounts'
    && view !== 'organization-wallet'
    && view !== 'wallet'
    && view !== 'subscriptions'
    && view !== 'invitations'
    && view !== 'models'
    && view !== 'playground'
    && view !== 'video-tasks'
    && view !== 'usage-logs'
    && view !== 'account-verification'
    && view !== 'profile'

  return (
    <SessionBoundary
      requireAdmin={requireAdmin}
      onSessionEnded={onLoginRequired}
      onUserHomeRequired={onUserHomeRequired}
    >
      {(session) => (
        <OrganizationWorkspaceProvider enterpriseView={view.startsWith('organization-') && view !== 'organization-provisioning'}>
          <AppShell title={t(titleKey)} currentUser={session.user} onLogout={onLogout}>
            {content(session.user.role === 'admin')}
          </AppShell>
        </OrganizationWorkspaceProvider>
      )}
    </SessionBoundary>
  )
}

export default function App() {
  const [route, navigate] = useHashRoute()
  const [loginNotice, setLoginNotice] = useState<LoginNotice>()
  const { view } = route

  const openLogin = useCallback((reason: 'required' | 'sessionExpired' | 'setupComplete') => {
    setLoginNotice(reason === 'required' ? undefined : reason)
    navigate('#/login')
  }, [navigate])

  const openConsole = useCallback(() => {
    setLoginNotice(undefined)
    navigate('#/console')
  }, [navigate])

  const openUserHome = useCallback(() => {
    setLoginNotice(undefined)
    navigate('#/console/models')
  }, [navigate])

  const openPasswordResetSuccess = useCallback(() => {
    setLoginNotice('passwordResetSuccess')
    navigate('#/login')
  }, [navigate])

  const openPasswordChanged = useCallback(() => {
    // 密码修改会递增会话版本，先清理本地旧令牌，再回到登录入口。
    endManagementSession()
    setLoginNotice('passwordChanged')
    navigate('#/login')
  }, [navigate])

  const logout = useCallback(() => {
    endManagementSession()
    setLoginNotice(undefined)
    navigate('#/login')
  }, [navigate])

  if (view === 'landing') {
    return <LandingPage />
  }
  if (view === 'api') {
    return <ApiExplorerPage />
  }
  if (view === 'public-models') {
    return <PublicModelPage />
  }
  if (view === 'register') {
    return <RegistrationPage inviteCode={route.inviteCode} onAuthenticated={openUserHome} />
  }
  if (view === 'forgot-password') {
    return <ForgotPasswordPage />
  }
  if (view === 'reset-password') {
    return <ResetPasswordPage token={route.resetToken} onCompleted={openPasswordResetSuccess} />
  }
  if (view === 'oauth-callback') {
    return <OAuthCallbackPage onAuthenticated={openConsole} />
  }
  if (view === 'share') {
    return <PublicPlaygroundSharePage key={route.shareToken ?? 'invalid'} token={route.shareToken} />
  }

  return (
    <ManagementEntry
      view={view}
      credentialChannelId={route.channelId}
      profileSection={route.profileSection}
      activityTab={route.activityTab}
      organizationId={route.organizationId}
      loginNotice={loginNotice}
      onAuthenticated={openConsole}
      onLoginRequired={openLogin}
      onUserHomeRequired={openUserHome}
      onPasswordChanged={openPasswordChanged}
      onLogout={logout}
    />
  )
}
