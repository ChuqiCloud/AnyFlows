import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminSiteSettings } from './site-settings-api'
import { SiteSettingsWorkspace } from './site-settings-workspace'
import { FrontendTemplatePanel } from './frontend-template-panel'
import { SiteNavigationPanel } from './site-navigation-panel'

/** 呈现管理员站点设置，并在固定记录不可用时保持失败关闭。 */
export function SiteSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminSiteSettings()

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('siteSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('siteSettings.subtitle')}
          </p>
        </div>
        <Button asChild size="sm" variant="secondary"><a href="#/console/system-settings/navigation">自定义导航</a></Button>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={settingsQuery.isFetching}
          onClick={() => void settingsQuery.refetch()}
        >
          <RefreshCw className={settingsQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('siteSettings.actions.refresh')}
        </Button>
      </header>

      {settingsQuery.isPending ? (
        <div
          className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.35fr)_minmax(17rem,0.65fr)]"
          aria-label={t('siteSettings.loading')}
        >
          <div className="overflow-hidden rounded-xl border border-[var(--hairline)]">
            <div className="border-b border-[var(--hairline)] p-4"><Skeleton className="h-5 w-40" /></div>
            <div className="grid gap-4 p-4 sm:grid-cols-2">
              {[0, 1, 2, 3, 4].map((item) => (
                <div key={item} className="grid gap-2">
                  <Skeleton className="h-3 w-24" />
                  <Skeleton className="h-9 w-full" />
                </div>
              ))}
            </div>
          </div>
          <Skeleton className="h-72 rounded-xl" />
        </div>
      ) : settingsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('siteSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('siteSettings.errors.load')}</p>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="mt-3"
            onClick={() => void settingsQuery.refetch()}
          >
            {t('siteSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data ? (
        <>
          <SiteSettingsWorkspace settings={settingsQuery.data} />
          <FrontendTemplatePanel />
        </>
      ) : null}
    </div>
  )
}

export function SiteNavigationPage() {
  const query = useAdminSiteSettings()
  return <div className="grid gap-4"><header><h2 className="text-lg font-semibold">自定义导航</h2><p className="mt-1 text-sm text-muted-foreground">管理顶栏、页脚和控制中心侧栏链接。</p></header>
    {query.isPending ? <p role="status">正在读取站点设置…</p> : query.isError ? <div role="alert">无法读取导航配置。<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>重试</Button></div> : query.data?.navigation ? <SiteNavigationPanel settings={query.data} /> : <p role="alert" className="text-destructive">当前服务接口未提供导航配置，请检查 API 服务版本和路由。</p>}
  </div>
}
