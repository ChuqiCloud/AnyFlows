import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { RibbonBeam } from '@/components/brand/ribbon-beam'
import { PublicSiteHeader } from '@/components/layout/public-site-header'
import type { PublicSiteSettings } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type AuthPageShellProps = {
  children: ReactNode
  className?: string
  site?: PublicSiteSettings
}

/** 为登录与会话状态提供一致的品牌背景，并复用公开页面顶栏。 */
export function AuthPageShell({ children, className, site }: AuthPageShellProps) {
  const { t } = useTranslation()
  const siteName = site?.site_name ?? t('brand.name')

  return (
    <div className="relative isolate min-h-dvh overflow-hidden bg-background text-foreground">
      <RibbonBeam className="-z-10 opacity-35" count={12} />

      <PublicSiteHeader siteName={siteName} logoUrl={site?.brand.logo_url} />

      <main
        className={cn(
          'mx-auto flex min-h-dvh w-full max-w-[1120px] items-center px-4 pt-20 pb-8 sm:px-6',
          className,
        )}
      >
        {children}
      </main>
    </div>
  )
}
