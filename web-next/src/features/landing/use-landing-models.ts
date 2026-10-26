import { useEffect, useState } from 'react'

import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { apiBaseUrl } from '@/lib/api/endpoint'
import { getManagementSessionToken, subscribeManagementSessionInvalidated } from '@/lib/api/session-token'

/** 首页和模型广场共用目录缓存及游客/登录价格边界。 */
export function useLandingModels() {
  const [authenticated, setAuthenticated] = useState(() => Boolean(getManagementSessionToken()))
  useEffect(() => {
    const unsubscribe = subscribeManagementSessionInvalidated(() => setAuthenticated(false))
    setAuthenticated(Boolean(getManagementSessionToken()))
    return unsubscribe
  }, [])
  const query = useModelCatalog('', EMPTY_MODEL_CATALOG_FILTERS, authenticated)
  return { query, authenticated, models: query.data?.pages[0]?.models ?? [] }
}

/** 复制示例始终指向当前部署所配置的公开地址。 */
export function useLandingBaseUrl() {
  const site = usePublicSiteSettings()
  return (site.data?.public_base_url || apiBaseUrl || window.location.origin).replace(/\/+$/, '')
}
