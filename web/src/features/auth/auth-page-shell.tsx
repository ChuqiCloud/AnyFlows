import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { RibbonBeam } from '@/components/brand/ribbon-beam'
import { LanguageToggle } from '@/components/language-toggle'
import { BrandMark } from '@/components/layout/brand-mark'
import { ThemeToggle } from '@/components/theme-toggle'
import { cn } from '@/lib/utils'

type AuthPageShellProps = {
  children: ReactNode
  className?: string
}

/** 为登录与会话状态提供一致的品牌背景和无障碍导航。 */
export function AuthPageShell({ children, className }: AuthPageShellProps) {
  const { t } = useTranslation()

  return (
    <div className="relative isolate min-h-dvh overflow-hidden bg-background text-foreground">
      <RibbonBeam className="-z-10 opacity-35" count={12} />

      <header className="absolute inset-x-0 top-0 z-20 border-b border-[var(--hairline)] bg-background/72 backdrop-blur-xl">
        <div className="mx-auto flex h-14 w-full max-w-[1120px] items-center justify-between px-4 sm:px-6">
          <a
            href="#/"
            className="flex items-center gap-2.5 rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-2 focus-visible:ring-offset-background"
            aria-label={t('shell.home')}
          >
            <BrandMark />
            <span className="text-[0.9375rem] font-semibold">{t('brand.name')}</span>
          </a>

          <div className="flex items-center gap-1.5">
            <LanguageToggle />
            <ThemeToggle />
          </div>
        </div>
      </header>

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
