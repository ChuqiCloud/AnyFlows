import { lazy, Suspense, type ReactNode } from 'react'
import { Navigate, useNavigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import type { RouteObject } from 'react-router-dom'

import { applyRouteGuards } from '@/shared/router'
import { forgeRoutes } from '@/feature/routes'
import { ConsoleLayout, useConsoleSession } from '@/routes/console-layout'
import { ConsoleExtensionPage } from '@/extensions/extension-page'
import { consoleExtensionRoutes } from '@/extensions/registry'
import { ManagementGate } from '@/routes/management-gate'

// 页面加载组件
function PageLoader() {
  return (
    <div className="flex min-h-[400px] items-center justify-center">
      <div className="size-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
    </div>
  )
}

// 懒加载页面组件
const LandingPage = lazy(() => import('@/features/landing/landing-page').then(m => ({ default: m.LandingPage })))
const PublicModelPage = lazy(() => import('@/features/models/public-model-page').then(m => ({ default: m.PublicModelPage })))
const ApiExplorerPage = lazy(() => import('@/features/api-explorer/api-explorer-page').then(m => ({ default: m.ApiExplorerPage })))
const RegistrationPage = lazy(() => import('@/features/registration/registration-page').then(m => ({ default: m.RegistrationPage })))
const SetupPage = lazy(() => import('@/features/setup/setup-page').then(m => ({ default: m.SetupPage })))
const ProfilePage = lazy(() => import('@/features/profile/profile-page').then(m => ({ default: m.ProfilePage })))
const PlaygroundPage = lazy(() => import('@/features/playground/playground-page').then(m => ({ default: m.PlaygroundPage })))
const PublicPlaygroundSharePage = lazy(() => import('@/features/playground/public-playground-share-page').then(m => ({ default: m.PublicPlaygroundSharePage })))
const RedemptionPage = lazy(() => import('@/features/redemptions/redemption-page').then(m => ({ default: m.RedemptionPage })))
const RefundPage = lazy(() => import('@/features/refunds/refund-page').then(m => ({ default: m.RefundPage })))
const RoutePage = lazy(() => import('@/features/routes/route-page').then(m => ({ default: m.RoutePage })))
const MySubscriptionsPage = lazy(() => import('@/features/subscriptions/my-subscriptions-page').then(m => ({ default: m.MySubscriptionsPage })))
const SubscriptionManagementPage = lazy(() => import('@/features/subscriptions/subscription-management-page').then(m => ({ default: m.SubscriptionManagementPage })))
const SiteSettingsPage = lazy(() => import('@/features/site-settings/site-settings-page').then(m => ({ default: m.SiteSettingsPage })))
const SiteNavigationPage = lazy(() => import('@/features/site-settings/site-settings-page').then(m => ({ default: m.SiteNavigationPage })))
const PaymentSettingsPage = lazy(() => import('@/features/payment-settings/payment-settings-page').then(m => ({ default: m.PaymentSettingsPage })))
const UserPage = lazy(() => import('@/features/users/user-page').then(m => ({ default: m.UserPage })))
const WalletPage = lazy(() => import('@/features/wallet/wallet-page').then(m => ({ default: m.WalletPage })))
const VideoTaskPage = lazy(() => import('@/features/video-tasks/video-task-page').then(m => ({ default: m.VideoTaskPage })))
const AccountVerificationPage = lazy(() => import('@/features/account-verification/account-verification-page').then(m => ({ default: m.AccountVerificationPage })))
const AuthenticationSettingsPage = lazy(() => import('@/features/authentication-settings/authentication-settings-page').then(m => ({ default: m.AuthenticationSettingsPage })))
const VerificationSettingsPanel = lazy(() => import('@/features/account-verification/verification-settings-panel').then(m => ({ default: m.VerificationSettingsPanel })))
const TokenPage = lazy(() => import('@/features/tokens/token-page').then(m => ({ default: m.TokenPage })))
const UsageLogPage = lazy(() => import('@/features/usage-logs/usage-log-page').then(m => ({ default: m.UsageLogPage })))

type ConsoleOutletContext = {
  onPasswordChanged: () => void
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
  { path: 'system-settings/navigation', element: <SiteNavigationPage /> },
  { path: 'system-settings/payment', element: <PaymentSettingsPage /> },
  { path: 'users', element: <UserPage /> },
  { path: 'wallet', element: <WalletPage /> },
  { path: 'playground', element: <PlaygroundPage /> },
  { path: 'video-tasks', element: <VideoTaskPage /> },
  { path: 'account-verification-reviews', element: <AccountVerificationPage admin /> },
  { path: 'system-settings/invitations', element: <AuthenticationSettingsPage rewardsOnly /> },
  { path: 'account-verification', element: <AccountVerificationPage /> },
  { path: 'system-settings/account-verification', element: <VerificationSettingsPanel /> },
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
  { index: true, element: <LandingPage /> },
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
          ...consoleExtensionRoutes.map((route) => ({
            path: route.path,
            element: <ConsoleExtensionPage path={route.path} />,
          })),
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
