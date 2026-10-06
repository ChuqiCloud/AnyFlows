import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import {
  getManagementSessionToken,
  subscribeManagementSessionInvalidated,
} from '@/lib/api/session-token'
import { ModelPage } from './model-page'

/** 为公开模型目录维护可选登录态；失效令牌清理后立即切换到游客查询。 */
function useOptionalSessionPresence() {
  const [hasSession, setHasSession] = useState(() => Boolean(getManagementSessionToken()))

  useEffect(() => {
    const unsubscribe = subscribeManagementSessionInvalidated(() => setHasSession(false))
    // 查询可能早于父组件 effect 返回 401；订阅后再同步一次可覆盖这个挂载竞态。
    setHasSession(Boolean(getManagementSessionToken()))
    return unsubscribe
  }, [])

  return hasSession
}

/** 游客与登录用户共用的公开模型广场外壳。 */
export function PublicModelPage() {
  const { t } = useTranslation()
  const hasSession = useOptionalSessionPresence()
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <PublicSiteHeader siteName={siteName} logoUrl={site?.brand.logo_url} />

      <main className="px-4 pt-19 pb-5 md:px-5 md:pt-21 md:pb-7">
        <div className="mx-auto w-full max-w-[1280px]">
          <ModelPage authenticated={hasSession} standalone />
        </div>
      </main>
      <SiteFooter siteName={siteName} logoUrl={site?.brand.logo_url} navigation={site?.navigation} />
    </div>
  )
}
