import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'

type NavigateOptions = { replace?: boolean }

type RouterLike = {
  navigate: (to: string, options?: NavigateOptions) => void
}

let activeRouter: RouterLike | null = null

/** 供非组件代码（支付回跳、脚本式跳转）触发站内跳转。 */
export function navigateTo(to: string, options?: NavigateOptions) {
  if (activeRouter) {
    activeRouter.navigate(to, options)
    return
  }

  window.location.assign(to)
}

/** 站内绝对路径才交给路由处理，API 与可下载资源保持浏览器默认行为。 */
function isRouterLink(href: string) {
  return href.startsWith('/') && !href.startsWith('//') && !href.startsWith('/api/') && !href.startsWith('/v1/')
}

function handleDocumentClick(event: MouseEvent) {
  if (event.defaultPrevented || event.button !== 0) return
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return

  const anchor = (event.target as Element | null)?.closest?.('a')
  if (!anchor) return
  if (anchor.target && anchor.target !== '_self') return
  if (anchor.hasAttribute('download')) return

  const href = anchor.getAttribute('href')
  if (!href || !isRouterLink(href)) return

  event.preventDefault()
  navigateTo(href)
}

/**
 * 把路由与页面里的同源链接接起来：
 * 业务里大量既有的 <a href="/console/..."> 无需改写即可获得 SPA 跳转体验。
 */
export function RouterNavigationBridge() {
  const navigate = useNavigate()

  useEffect(() => {
    activeRouter = { navigate: (to, options) => navigate(to, options) }
    document.addEventListener('click', handleDocumentClick)

    return () => {
      activeRouter = null
      document.removeEventListener('click', handleDocumentClick)
    }
  }, [navigate])

  return null
}
