import { AlertTriangle, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { BrandMark } from '@/components/layout/brand-mark'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { AuthPageShell } from '@/features/auth/auth-page-shell'

/** 首次安装状态读取期间保持固定布局，避免登录页短暂闪现。 */
export function SetupGateLoading() {
  const { t } = useTranslation()

  return (
    <AuthPageShell className="justify-center">
      <div className="w-full max-w-[360px]" role="status" aria-live="polite">
        <div className="mb-5 flex items-center gap-3">
          <BrandMark />
          <div className="grid flex-1 gap-2">
            <Skeleton className="h-3.5 w-28" />
            <Skeleton className="h-2.5 w-40" />
          </div>
        </div>
        <Skeleton className="h-1 w-full" />
        <span className="sr-only">{t('setup.gate.loading')}</span>
      </div>
    </AuthPageShell>
  )
}

/** 状态读取失败时只提供显式重试，不假定系统已经安装。 */
export function SetupGateUnavailable({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()

  return (
    <AuthPageShell className="justify-center">
      <Card elevation="overlay" className="w-full max-w-[420px] bg-card/94 backdrop-blur-2xl">
        <CardHeader className="gap-2 p-6 pb-4">
          <div className="mb-2 grid size-10 place-items-center rounded-xl border border-warning/20 bg-warning/10 text-warning">
            <AlertTriangle className="size-5" aria-hidden="true" />
          </div>
          <CardTitle className="text-lg leading-tight">{t('setup.gate.unavailableTitle')}</CardTitle>
          <CardDescription className="leading-6">{t('setup.gate.unavailableBody')}</CardDescription>
        </CardHeader>
        <CardContent className="px-6 pb-6">
          <Button type="button" onClick={onRetry}>
            <RefreshCw className="size-4" aria-hidden="true" />
            {t('setup.gate.retry')}
          </Button>
        </CardContent>
      </Card>
    </AuthPageShell>
  )
}
