import { AlertTriangle, RefreshCw } from 'lucide-react'
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Card, CardBody, CardHeader, Skeleton } from '@heroui/react'

import { BrandMark } from '@/components/layout/brand-mark'
import { cn } from '@/lib/utils'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { endManagementSession, useManagementSession } from '@/features/auth/session-query'
import type { SessionResponse } from '@/lib/api/generated/types.gen'
import {
  getManagementSessionToken,
  subscribeManagementSessionInvalidated,
} from '@/lib/api/session-token'

type SessionEndReason = 'required' | 'sessionExpired'

/*
 * 卡片外观沿用原设计系统：HeroUI Card 自带 bg-content1 / h-auto，
 * 这里覆盖回 1px 细边界；圆角直接用 HeroUI 的 rounded-large（同为 14px）。
 */
const cardClass = 'border border-[var(--hairline)] bg-card text-card-foreground'

// 骨架沿用原占位外观：HeroUI Skeleton 自带渐变 shimmer，关掉动画后回退为 animate-pulse 底色。
const skeletonClass = 'animate-pulse rounded-md bg-muted'

// 按钮基准沿用原设计系统：覆盖 HeroUI 的 font-normal / min-w-max 与自带高度。
const buttonBase = 'min-w-0 rounded-lg font-medium'
const buttonPrimary = cn(buttonBase, 'h-9 gap-1.5 px-3.5 hover:bg-[var(--primary-hover)]')
const buttonLight = cn(buttonBase, 'h-9 gap-1.5 px-3.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground')

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
            <Skeleton disableAnimation className={cn(skeletonClass, 'h-3.5 w-24')} />
            <Skeleton disableAnimation className={cn(skeletonClass, 'h-2.5 w-36')} />
          </div>
        </div>
        <Skeleton disableAnimation className={cn(skeletonClass, 'h-1 w-full')} />
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
      <Card shadow="none" className={cn(cardClass, 'w-full max-w-[420px] bg-card/94 backdrop-blur-2xl')}>
        {/* HeroUI 头部默认横排 + items-center + p-3，补回原竖排与 p-5 基准。 */}
        <CardHeader className="flex-col items-stretch gap-1.5 p-5 gap-2 p-6 pb-4">
          <div className="mb-2 grid size-10 place-items-center rounded-xl border border-warning/20 bg-warning/10 text-warning">
            <AlertTriangle className="size-5" aria-hidden="true" />
          </div>
          <h3 data-slot="card-title" className="text-[0.9375rem] leading-none font-semibold text-lg leading-tight">{t('auth.session.unavailableTitle')}</h3>
          <p data-slot="card-description" className="text-sm text-muted-foreground leading-6">{t('auth.session.unavailableBody')}</p>
        </CardHeader>
        <CardBody className="p-5 pt-0 flex flex-col gap-2 px-6 pb-6 sm:flex-row">
          <Button data-slot="button" type="button" className={buttonPrimary} onPress={onRetry}>
            <RefreshCw className="size-4" aria-hidden="true" />
            {t('auth.session.retry')}
          </Button>
          <Button data-slot="button" type="button" variant="light" className={buttonLight} onPress={onUseOtherAccount}>
            {t('auth.session.otherAccount')}
          </Button>
        </CardBody>
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
