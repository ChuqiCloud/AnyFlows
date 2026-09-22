import { useRoutes } from 'react-router-dom'

import { routes } from '@/routes'
import { AppShell } from '@/shared/app-shell'

/**
 * 入口外壳：平台 AppShell 包裹全站路由表。
 * 模块路由已由 routes/index.tsx 按挂载父级（控制台 / 管理边界）嵌套，
 * 这里不能再把它们平铺到顶层——那样会绕过 ConsoleLayout，丢掉会话上下文。
 */
export function App() {
  const element = useRoutes(routes)

  return <AppShell>{element}</AppShell>
}
