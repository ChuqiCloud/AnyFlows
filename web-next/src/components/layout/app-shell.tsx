import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router-dom'

import { useAppNavigationItems } from '@/components/layout/app-navigation'
import { NavigationSectionSwitcher } from '@/components/layout/navigation-section-switcher'
import { AnnouncementStrip } from '@/features/announcements/announcement-strip'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import type { SessionUser } from '@/lib/api/generated/types.gen'
import { findSidebarSectionKeyByPath, ModuleShell, ModuleSidebarLayoutContainer } from '@/shared/app-shell'
import { Navbar } from '@/shared/nav'

/** 站点名取不到时的兜底品牌名。 */
const FALLBACK_SITE_NAME = 'AnyFlows'

/*
 * 侧栏表面用页面底色，而不是 acmeidc 原件默认的 bg-content1（HeroUI 的内容面板色）：
 * 内容区一短，侧栏整列的底色会和下方页面底色直接并排，两种白看上去就很突兀。
 * acmeidc 的浅色主题里 background 与 content1 同为纯白，本来就不存在这个差异。
 * 层次改由边框和卡片（bg-card）承担。
 */
const SIDEBAR_CONTAINER_CLASS = 'h-full border-r border-divider w-60 flex flex-col bg-background'

/*
 * AnyFlows 原实现里分组标题与条目图标左对齐；HeroUI 的 ListboxSection 默认标题缩进是 pl-1，
 * 比条目的内边距少 8px，标题会贴到侧栏最左侧。这里补成与条目一致的 12px。
 */
const SIDEBAR_SECTION_CLASSES = { heading: 'pl-3' }

type AppShellProps = {
  children: ReactNode
  currentUser: SessionUser
  onLogout: () => void
}

/** 控制台外壳直接挂载 acmeidc 原件：Header 用 Navbar，侧栏用 ModuleSidebarLayoutContainer。 */
export function AppShell({ children, currentUser, onLogout }: AppShellProps) {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  const navigate = useNavigate()
  const items = useAppNavigationItems(currentUser.role)
  const siteQuery = usePublicSiteSettings()
  // 侧栏标题与 Header 品牌同名：属于外壳级常量，不随页面变化。
  const title = siteQuery.data?.site_name ?? FALLBACK_SITE_NAME

  /*
   * 侧栏内容完全由路由派生，不额外存状态：选板块直接进那个板块的首个页面，
   * 侧栏与正文不会出现“看着是 A 板块、内容却是 B 页面”的错位。
   */
  const activeSectionKey = findSidebarSectionKeyByPath(items, pathname)
  const openSection = (key: string) => {
    const href = items.find((item) => item.key === key)?.children?.[0]?.href

    if (href) {
      navigate(href)
    }
  }

  return (
    <ModuleShell
      /*
       * 内容区限宽 960px 居中，与旧站一致：
       * 控制台页面是按窄栏排版设计的，不设限宽时表格和表单会被拉到整个视口宽。
       * 内边距留在滚动容器上，限宽只作用于正文，这样滚动时留白不会跟着跳。
       */
      contentClassName="min-w-0 flex-1 overflow-auto px-4 py-4 md:px-5"
      navbar={<Navbar currentUser={currentUser} onLogout={onLogout} />}
      sidebar={
        <ModuleSidebarLayoutContainer
          containerClassName={SIDEBAR_CONTAINER_CLASS}
          defaultSelectedKey="overview"
          headerAction={(
            <NavigationSectionSwitcher
              ariaLabel={t('nav.sectionSwitcher')}
              sections={items.map((item) => ({ key: item.key, title: item.title }))}
              value={activeSectionKey}
              onChange={openSection}
            />
          )}
          items={items.filter((item) => item.key === activeSectionKey)}
          sectionClasses={SIDEBAR_SECTION_CLASSES}
          title={title}
        />
      }
    >
      <AnnouncementStrip />
      <div className="mx-auto w-full max-w-[960px]">{children}</div>
    </ModuleShell>
  )
}
