import { AlertTriangle, RefreshCw } from 'lucide-react'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BrandMark } from '@/components/layout/brand-mark'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { endManagementSession, useManagementSession } from '@/features/auth/session-query'
import type { SessionResponse } from '@/lib/api/generated/types.gen'
import {
  getManagementSessionToken,
  subscribeManagementSessionInvalidated,
} from '@/lib/api/session-token'

type SessionEndReason = 'required' | 'sessionExpired'

type SessionBoundaryProps = {
  children: (session: SessionResponse) => ReactNode
  requireAdmin: boolean
  onSessionEnded: (reason: SessionEndReason) => void
  onUserHomeRequired: () => void
}

function SessionLoading() {
  const { t } = useTranslation()

  return (
    <AuthPageShell className="justify-center">
      <div className="w-full max-w-[360px]" role="status" aria-live="polite">
        <div className="mb-5 flex items-center gap-3">
          <BrandMark />
          <div className="grid flex-1 gap-2">
            <Skeleton className="h-3.5 w-24" />
            <Skeleton className="h-2.5 w-36" />
          </div>
        </div>
        <Skeleton className="h-1 w-full" />
        <span className="sr-only">{t('auth.session.loading')}</span>
      </div>
    </AuthPageShell>
  )
}

function SessionUnavailable({ onRetry, onUseOtherAccount }: {
  onRetry: () => void
  onUseOtherAccount: () => void
}) {
  const { t } = useTranslation()

  return (
    <AuthPageShell className="justify-center">
      <Card elevation="overlay" className="w-full max-w-[420px] bg-card/94 backdrop-blur-2xl">
        <CardHeader className="gap-2 p-6 pb-4">
          <div className="mb-2 grid size-10 place-items-center rounded-xl border border-warning/20 bg-warning/10 text-warning">
            <AlertTriangle className="size-5" aria-hidden="true" />
          </div>
          <CardTitle className="text-lg leading-tight">{t('auth.session.unavailableTitle')}</CardTitle>
          <CardDescription className="leading-6">{t('auth.session.unavailableBody')}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2 px-6 pb-6 sm:flex-row">
          <Button type="button" onClick={onRetry}>
            <RefreshCw className="size-4" aria-hidden="true" />
            {t('auth.session.retry')}
          </Button>
          <Button type="button" variant="ghost" onClick={onUseOtherAccount}>
            {t('auth.session.otherAccount')}
          </Button>
        </CardContent>
      </Card>
    </AuthPageShell>
  )
}

/** 在挂载控制台前恢复服务端会话，并集中处理角色与失效边界。 */
export function SessionBoundary({
  children,
  requireAdmin,
  onSessionEnded,
  onUserHomeRequired,
}: SessionBoundaryProps) {
  const initialHasToken = useRef(Boolean(getManagementSessionToken()))
  const [hasToken, setHasToken] = useState(initialHasToken.current)
  const sessionQuery = useManagementSession(hasToken)

  useEffect(() => {
    if (!initialHasToken.current) {
      onSessionEnded('required')
    }
  }, [onSessionEnded])

  useEffect(
    () =>
      subscribeManagementSessionInvalidated(() => {
        setHasToken(false)
        onSessionEnded('sessionExpired')
      }),
    [onSessionEnded],
  )

  useEffect(() => {
    if (!requireAdmin || sessionQuery.data?.user.role !== 'user') {
      return
    }

    // 普通用户保留有效会话并回到用户入口，避免误清理刚签发的令牌。
    onUserHomeRequired()
  }, [onUserHomeRequired, requireAdmin, sessionQuery.data])

  if (
    !hasToken
    || sessionQuery.isPending
    || (requireAdmin && sessionQuery.data?.user.role === 'user')
  ) {
    return <SessionLoading />
  }

  if (sessionQuery.isError) {
    return (
      <SessionUnavailable
        onRetry={() => void sessionQuery.refetch()}
        onUseOtherAccount={() => {
          endManagementSession()
          setHasToken(false)
          onSessionEnded('required')
        }}
      />
    )
  }

  return children(sessionQuery.data)
}
