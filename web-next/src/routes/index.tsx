import { AccountVerificationPage } from '@/features/account-verification/account-verification-page'
import type { ReactNode } from 'react'
import { Navigate, useNavigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import type { RouteObject } from 'react-router-dom'

import { PublicModelPage } from '@/features/models/public-model-page'
import { ApiExplorerPage } from '@/features/api-explorer/api-explorer-page'
import { OrganizationActivityPage } from '@/features/organization-activity/organization-activity-page'
import { OrganizationApiKeyPage } from '@/features/organization-api-keys/organization-api-key-page'
import { OrganizationApprovalPage } from '@/features/organization-approvals/organization-approval-page'
import { OrganizationBudgetPage } from '@/features/organization-budgets/organization-budget-page'
import { OrganizationContractPricePage } from '@/features/organization-contract-prices/organization-contract-price-page'
import { OrganizationOverviewPage } from '@/features/organization-console/organization-overview-page'
import { OrganizationPlanManagementPage } from '@/features/organization-console/organization-plan-management-page'
import { OrganizationSettingsPage } from '@/features/organization-console/organization-settings-page'
import { OrganizationCreditPage } from '@/features/organization-credit/organization-credit-page'
import { OrganizationDepartmentPage } from '@/features/organization-departments/organization-department-page'
import { OrganizationMemberPage } from '@/features/organization-members/organization-member-page'
import { OrganizationOnboardingPage } from '@/features/organization-onboarding/organization-onboarding-page'
import { OrganizationProvisioningPage } from '@/features/organization-provisioning/organization-provisioning-page'
import { OrganizationServiceAccountPage } from '@/features/organization-service-accounts/organization-service-account-page'
import { OrganizationVerificationPage } from '@/features/organization-verification/organization-verification-page'
import { OrganizationWalletPage } from '@/features/organization-wallet/organization-wallet-page'
import { PaymentSettingsPage } from '@/features/payment-settings/payment-settings-page'
import { PlatformOrganizationPage } from '@/features/platform-organizations/platform-organization-page'
import { PlaygroundPage } from '@/features/playground/playground-page'
import { PublicPlaygroundSharePage } from '@/features/playground/public-playground-share-page'
import { ProfilePage } from '@/features/profile/profile-page'
import { RedemptionPage } from '@/features/redemptions/redemption-page'
import { RefundPage } from '@/features/refunds/refund-page'
import { RegistrationPage } from '@/features/registration/registration-page'
import { RoutePage } from '@/features/routes/route-page'
import { SetupPage } from '@/features/setup/setup-page'
import { SiteSettingsPage } from '@/features/site-settings/site-settings-page'
import { MySubscriptionsPage } from '@/features/subscriptions/my-subscriptions-page'
import { SubscriptionManagementPage } from '@/features/subscriptions/subscription-management-page'
import { TokenPage } from '@/features/tokens/token-page'
import { UsageLogPage } from '@/features/usage-logs/usage-log-page'
import { UserPage } from '@/features/users/user-page'
import { VideoTaskPage } from '@/features/video-tasks/video-task-page'
import { WalletPage } from '@/features/wallet/wallet-page'
import { applyRouteGuards } from '@/shared/router'
import { forgeRoutes } from '@/feature/routes'
import { ConsoleLayout, useConsoleSession } from '@/routes/console-layout'
import { ManagementGate } from '@/routes/management-gate'

type ConsoleOutletContext = {
  onPasswordChanged: () => void
}

/** 只接受单个十进制正整数查询参数，避免宽松转换产生错误深链。 */
function positiveIntegerParam(params: URLSearchParams, key: string) {
  const values = params.getAll(key)

  if (values.length !== 1 || !/^[1-9]\d*$/.test(values[0])) return undefined

  const value = Number(values[0])
  return Number.isSafeInteger(value) ? value : undefined
}

function SetupRoute() {
  const navigate = useNavigate()

  return (
    <SetupPage
      onAuthenticated={() => navigate('/console', { replace: true })}
      onConflict={() => navigate('/login', { replace: true, state: { notice: 'setupComplete' } })}
    />
  )
}

function RegistrationRoute() {
  const navigate = useNavigate()
  const [params] = useSearchParams()

  return (
    <RegistrationPage
      inviteCode={params.get('invite') ?? undefined}
      onAuthenticated={() => navigate('/console/models', { replace: true })}
    />
  )
}

function PlaygroundShareRoute() {
  const { token } = useParams()

  return <PublicPlaygroundSharePage key={token ?? 'invalid'} token={token} />
}

function OrganizationActivityRoute() {
  const [params] = useSearchParams()
  const tab = params.get('tab')

  return (
    <OrganizationActivityPage
      initialTab={tab === 'audit' || tab === 'scim' || tab === 'usage' ? tab : undefined}
    />
  )
}

function OrganizationOnboardingRoute() {
  const [params] = useSearchParams()

  return (
    <OrganizationOnboardingPage
      requestedOrganizationId={positiveIntegerParam(params, 'organization_id')}
    />
  )
}

function ProfileRoute() {
  const navigate = useNavigate()
  const { onPasswordChanged } = useOutletContext<ConsoleOutletContext>()
  const [params] = useSearchParams()

  return (
    <ProfilePage
      section={params.get('section') === 'notifications' ? 'notifications' : undefined}
      onPasswordChanged={() => {
        onPasswordChanged()
        navigate('/login', { replace: true, state: { notice: 'passwordChanged' } })
      }}
    />
  )
}

function UsageLogRoute() {
  const session = useConsoleSession()

  return <UsageLogPage admin={session.user.role === 'admin'} />
}

function OrganizationProvisioningRoute() {
  const session = useConsoleSession()

  return <OrganizationProvisioningPage admin={session.user.role === 'admin'} />
}

function OrganizationVerificationRoute() {
  const session = useConsoleSession()

  return <OrganizationVerificationPage admin={session.user.role === 'admin'} />
}

type ConsoleRouteDefinition = {
  path: string
  element: ReactNode
}

/** 控制台路由表：路径即真实 URL。 */
const consoleRoutes: ConsoleRouteDefinition[] = [
  { path: 'redemption-codes', element: <RedemptionPage /> },
  { path: 'refunds', element: <RefundPage /> },
  { path: 'routes', element: <RoutePage /> },
  { path: 'subscriptions', element: <MySubscriptionsPage /> },
  {
    path: 'subscription-management',
    element: <SubscriptionManagementPage />,
  },
  { path: 'system-settings/site', element: <SiteSettingsPage /> },
  { path: 'system-settings/payment', element: <PaymentSettingsPage /> },
  { path: 'users', element: <UserPage /> },
  { path: 'organization', element: <OrganizationOverviewPage /> },
  {
    path: 'organization-api-keys',
    element: <OrganizationApiKeyPage />,
  },
  {
    path: 'organization-activity',
    element: <OrganizationActivityRoute />,
  },
  { path: 'organization-budgets', element: <OrganizationBudgetPage /> },
  {
    path: 'organization-contract-prices',
    element: <OrganizationContractPricePage />,
  },
  { path: 'organization-credit', element: <OrganizationCreditPage /> },
  {
    path: 'organization-approvals',
    element: <OrganizationApprovalPage />,
  },
  { path: 'organization-members', element: <OrganizationMemberPage /> },
  {
    path: 'organization-departments',
    element: <OrganizationDepartmentPage />,
  },
  { path: 'organization-wallet', element: <OrganizationWalletPage /> },
  {
    path: 'organization-onboarding',
    element: <OrganizationOnboardingRoute />,
  },
  { path: 'organization-plans', element: <OrganizationPlanManagementPage /> },
  {
    path: 'organization-provisioning',
    element: <OrganizationProvisioningRoute />,
  },
  {
    path: 'organization-verification',
    element: <OrganizationVerificationRoute />,
  },
  { path: 'organization-settings', element: <OrganizationSettingsPage /> },
  {
    path: 'organization-service-accounts',
    element: <OrganizationServiceAccountPage />,
  },
  { path: 'platform-organizations', element: <PlatformOrganizationPage /> },
  { path: 'wallet', element: <WalletPage /> },
  { path: 'playground', element: <PlaygroundPage /> },
  { path: 'video-tasks', element: <VideoTaskPage /> },
  { path: 'account-verification', element: <AccountVerificationPage /> },
  { path: 'profile', element: <ProfileRoute /> },
  { path: 'tokens', element: <TokenPage /> },
  { path: 'usage-logs', element: <UsageLogRoute /> },
]

/**
 * 全站路由表。控制台位于 /console 之下，公开页各占一个顶级路径，
 * 首次安装与登录边界由 ManagementGate 与控制台布局统一承担。
 */
/**
 * 模块路由按平台契约声明为 AppRouteObject，运行时与 react-router 的 RouteObject 同构，
 * 只多出平台自己的 meta（当前由控制台布局按路径解析），此处做一次类型收敛。
 */
const moduleRoutes = applyRouteGuards(forgeRoutes)

/**
 * 模块路由按挂载父级分组：控制台模块挂在 /console 之下，
 * 公开入口（登录、找回密码、OAuth 回调）挂在管理边界之下。
 */
const consoleModuleRoutes = moduleRoutes.filter((route) => route.path?.startsWith('/console/'))
// 控制台首页由 dashboard 模块声明为绝对路径 /console，这里还原成 index 子路由。
const consoleIndexModuleRoutes = moduleRoutes.filter((route) => route.path === '/console')
const gateModuleRoutes = moduleRoutes.filter(
  (route) => route.path !== '/console' && !route.path?.startsWith('/console/'),
)

export const routes: RouteObject[] = [
  // 首页（landing）暂缓迁移，先落到控制台避免空路由。
  { index: true, element: <Navigate to="/console" replace /> },
  { path: 'models', element: <PublicModelPage /> },
  { path: 'api', element: <ApiExplorerPage /> },
  { path: 'register', element: <RegistrationRoute /> },
  { path: 'share/:token', element: <PlaygroundShareRoute /> },
  {
    element: <ManagementGate />,
    children: [
      { path: 'setup', element: <SetupRoute /> },
      ...gateModuleRoutes,
      {
        path: 'console',
        element: <ConsoleLayout />,
        children: [
          ...consoleIndexModuleRoutes.map((route) => ({ index: true, element: route.element })),
          // 已迁移到模块的路由优先于旧业务路由，避免同路径时旧表先声明而胜出。
          ...consoleModuleRoutes,
          ...consoleRoutes.map((route) => ({
            path: route.path,
            element: route.element,
          })),
        ],
      },
    ],
  },
  { path: '*', element: <Navigate to="/console" replace /> },
]
