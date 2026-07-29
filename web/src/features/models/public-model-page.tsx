import { LayoutDashboard, LogIn } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BrandMark } from '@/components/layout/brand-mark'
import { LanguageToggle } from '@/components/language-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import { Button } from '@/components/ui/button'
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
  const AccountIcon = hasSession ? LayoutDashboard : LogIn

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <header className="sticky top-0 z-30 border-b border-[var(--hairline)] bg-background/85 backdrop-blur-xl">
        <div className="mx-auto flex h-14 max-w-[1040px] items-center justify-between gap-3 px-3 sm:px-5">
          <a
            href="#/"
            className="flex min-w-0 items-center gap-2.5 rounded-lg focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
          >
            <BrandMark />
            <div className="hidden min-w-0 sm:block">
              <div className="truncate text-sm font-semibold">{t('brand.name')}</div>
              <div className="truncate text-[0.625rem] text-muted-foreground">
                {t('models.public.scope')}
              </div>
            </div>
          </a>

          <div className="flex shrink-0 items-center gap-1.5">
            <LanguageToggle />
            <ThemeToggle />
            <Button size="sm" asChild>
              <a href={hasSession ? '#/console/models' : '#/login'}>
                <AccountIcon className="size-3.5" aria-hidden="true" />
                {t(hasSession ? 'models.public.console' : 'models.public.login')}
              </a>
            </Button>
          </div>
        </div>
      </header>

      <main className="px-4 py-5 md:px-5 md:py-7">
        <div className="mx-auto w-full max-w-[960px]">
          <ModelPage authenticated={hasSession} standalone />
        </div>
      </main>
    </div>
  )
}
