import type { ReactNode } from 'react'
import { createElement } from 'react'
import { Navigate, useNavigate, useOutletContext, useParams, useSearchParams } from 'react-router-dom'
import type { RouteObject } from 'react-router-dom'

// 路由表只保留控制台/引导页这类体积小、首次交互必需的模块做同步导入；
// 公开页（首页、模型广场、API 文档、登录注册、广场分享）全部走 lazyRoute，
// 否则首屏会把这些页面的依赖（图标集、Markdown、支付 SDK）一并下载。
import { PublicPlaygroundSharePage } from '@/features/playground/public-playground-share-page'
import { applyRouteGuards, lazyComponent, lazyRoute } from '@/shared/router'
import { forgeRoutes } from '@/feature/routes'
import { ConsoleLayout, useConsoleSession } from '@/routes/console-layout'
import { ConsoleExtensionPage } from '@/extensions/extension-page'
import { consoleExtensionRoutes } from '@/extensions/registry'
import { ManagementGate } from '@/routes/management-gate'

type ConsoleOutletContext = {
  onPasswordChanged: () => void
}

/* 公开页与控制台旧页都按需加载：首屏只跑控制台骨架，页面代码随访问路径拉取。 */
const lazyLandingPage = lazyRoute(() => import('@/features/landing/landing-page'), 'LandingPage')
const lazyPublicModelPage = lazyRoute(() => import('@/features/models/public-model-page'), 'PublicModelPage')
const lazyApiExplorerPage = lazyRoute(() => import('@/features/api-explorer/api-explorer-page'), 'ApiExplorerPage')
const lazySetupPage = lazyComponent(() => import('@/features/setup/setup-page'), 'SetupPage')
const lazyRegistrationPage = lazyComponent(() => import('@/features/registration/registration-page'), 'RegistrationPage')
const lazyProfilePage = lazyComponent(() => import('@/features/profile/profile-page'), 'ProfilePage')
const lazyUsageLogPage = lazyComponent(() => import('@/features/usage-logs/usage-log-page'), 'UsageLogPage')
const lazyRedemptionPage = lazyRoute(() => import('@/features/redemptions/redemption-page'), 'RedemptionPage')
const lazyRefundPage = lazyRoute(() => import('@/features/refunds/refund-page'), 'RefundPage')
const lazyRoutePage = lazyRoute(() => import('@/features/routes/route-page'), 'RoutePage')
const lazyPaymentSettingsPage = lazyRoute(() => import('@/features/payment-settings/payment-settings-page'), 'PaymentSettingsPage')
const lazyUserPage = lazyRoute(() => import('@/features/users/user-page'), 'UserPage')
const lazyWalletPage = lazyRoute(() => import('@/features/wallet/wallet-page'), 'WalletPage')
const lazyPlaygroundPage = lazyRoute(() => import('@/features/playground/playground-page'), 'PlaygroundPage')
const lazyVideoTaskPage = lazyRoute(() => import('@/features/video-tasks/video-task-page'), 'VideoTaskPage')
const lazyTokenPage = lazyRoute(() => import('@/features/tokens/token-page'), 'TokenPage')
const AccountVerificationPage = lazyComponent(() => import('@/features/account-verification/account-verification-page'), 'AccountVerificationPage')
const VerificationSettingsPanel = lazyRoute(() => import('@/features/account-verification/verification-settings-panel'), 'VerificationSettingsPanel')
const AboutPage = lazyRoute(() => import('@/features/about/about-page'), 'AboutPage')
const lazyAuthenticationSettingsPage = lazyComponent(() => import('@/features/authentication-settings/authentication-settings-page'), 'AuthenticationSettingsPage')
const lazySubscriptionManagementPage = lazyRoute(() => import('@/features/subscriptions/subscription-management-page'), 'SubscriptionManagementPage')
const SiteSettingsPage = lazyRoute(() => import('@/features/site-settings/site-settings-page'), 'SiteSettingsPage')
const SiteNavigationPage = lazyRoute(() => import('@/features/site-settings/site-settings-page'), 'SiteNavigationPage')

function SetupRoute() {
  const navigate = useNavigate()

  // 需要传递 props 的页面用 createElement 挂载，模块本身仍按需加载。
  return createElement(lazySetupPage, {
    onAuthenticated: () => navigate('/console', { replace: true }),
    onConflict: () => navigate('/login', { replace: true, state: { notice: 'setupComplete' } }),
  })
}

function RegistrationRoute() {
  const navigate = useNavigate()
  const [params] = useSearchParams()

  return createElement(lazyRegistrationPage, {
    inviteCode: params.get('invite') ?? undefined,
    onAuthenticated: () => navigate('/console', { replace: true }),
  })
}

function PlaygroundShareRoute() {
  const { token } = useParams()

  return <PublicPlaygroundSharePage key={token ?? 'invalid'} token={token} />
}

function ProfileRoute() {
  const navigate = useNavigate()
  const { onPasswordChanged } = useOutletContext<ConsoleOutletContext>()
  const [params] = useSearchParams()

  return createElement(lazyProfilePage, {
    section: params.get('section') === 'notifications' ? 'notifications' : undefined,
    onPasswordChanged: () => {
      onPasswordChanged()
      navigate('/login', { replace: true, state: { notice: 'passwordChanged' } })
    },
  })
}

function UsageLogRoute() {
  const session = useConsoleSession()

  return createElement(lazyUsageLogPage, { admin: session.user.role === 'admin' })
}

type ConsoleRouteDefinition = {
  path: string
  element: ReactNode
}

/** 控制台路由表：路径即真实 URL。 */
const consoleRoutes: ConsoleRouteDefinition[] = [
  { path: 'redemption-codes', element: lazyRedemptionPage },
  { path: 'refunds', element: lazyRefundPage },
  { path: 'routes', element: lazyRoutePage },
  { path: 'subscriptions', element: <Navigate to="/console/wallet" replace /> },
  {
    path: 'subscription-management',
    element: lazySubscriptionManagementPage,
  },
  { path: 'system-settings/site', element: SiteSettingsPage },
  { path: 'system-settings/about', element: AboutPage },
  { path: 'system-settings/navigation', element: SiteNavigationPage },
  { path: 'system-settings/payment', element: lazyPaymentSettingsPage },
  { path: 'users', element: lazyUserPage },
  { path: 'wallet', element: lazyWalletPage },
  { path: 'playground', element: lazyPlaygroundPage },
  { path: 'video-tasks', element: lazyVideoTaskPage },
  { path: 'account-verification-reviews', element: createElement(AccountVerificationPage, { admin: true }) },
  { path: 'system-settings/invitations', element: createElement(lazyAuthenticationSettingsPage, { rewardsOnly: true }) },
  { path: 'account-verification', element: createElement(AccountVerificationPage) },
  { path: 'system-settings/account-verification', element: VerificationSettingsPanel },
  { path: 'profile', element: <ProfileRoute /> },
  { path: 'tokens', element: lazyTokenPage },
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
  { index: true, element: lazyLandingPage },
  { path: 'models', element: lazyPublicModelPage },
  { path: 'api', element: lazyApiExplorerPage },
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
