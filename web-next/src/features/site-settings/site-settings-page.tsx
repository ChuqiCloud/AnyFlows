import { Button, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Link } from 'react-router-dom'

import { useAdminSiteSettings } from './site-settings-api'
import { FrontendTemplatePanel } from './frontend-template-panel'
import { SiteSettingsWorkspace } from './site-settings-workspace'
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
        <Button as={Link} to="/console/system-settings/navigation" size="sm" variant="flat">自定义导航</Button>
        <Button
          type="button"
          size="sm"
          variant="bordered"
          isDisabled={settingsQuery.isFetching}
          onClick={() => void settingsQuery.refetch()}
        >
          <RefreshCw className={settingsQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
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
            variant="bordered"
            className="mt-3"
            onClick={() => void settingsQuery.refetch()}
          >
            {t('siteSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data ? (
        <div className="grid gap-4"><FrontendTemplatePanel /><SiteSettingsWorkspace settings={settingsQuery.data} /></div>
      ) : null}
    </div>
  )
}

export function SiteNavigationPage() {
  const query = useAdminSiteSettings()
  return <div className="grid gap-4"><header><h2 className="text-lg font-semibold">自定义导航</h2><p className="mt-1 text-sm text-default-500">管理顶栏、页脚和控制中心侧栏链接。</p></header>
    {query.isPending ? <p role="status">正在读取站点设置…</p> : query.isError ? <div role="alert" className="flex items-center gap-2">无法读取导航配置。<Button size="sm" onPress={() => void query.refetch()}>重试</Button></div> : query.data?.navigation ? <SiteNavigationPanel settings={query.data} /> : <p role="alert" className="text-danger">当前服务接口未提供导航配置，请检查 API 服务版本和路由。</p>}
  </div>
}
