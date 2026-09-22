import { AlertCircle, CheckCircle2, LoaderCircle, RotateCcw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Card, CardBody, CardHeader } from '@heroui/react'

import { cn } from '@/lib/utils'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { exchangeOAuthTicket } from '@/features/auth/oauth-login-api'
import { establishManagementSession } from '@/features/auth/session-query'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'

type OAuthCallbackPageProps = {
  onAuthenticated: () => void
}

type CallbackPayload =
  | { kind: 'ticket'; ticket: string }
  | { kind: 'cancelled' }
  | { kind: 'failed' }

type CallbackStatus = 'exchanging' | 'succeeded' | 'cancelled' | 'failed'

const ticketPattern = /^[A-Za-z0-9_-]{43}$/
let consumedCallbackPayload: CallbackPayload | undefined

/*
 * 卡片与按钮外观沿用原设计系统：HeroUI 自带的内边距、字号与间距由 className 覆盖回原值。
 * 圆角无需处理——HeroUI 的 rounded-large / rounded-medium 分别为 14px / 12px，与原设计一致。
 */
const cardClass = 'border border-[var(--hairline)] bg-card text-card-foreground'
const buttonBase = 'min-w-0 rounded-lg font-medium [&_svg:not([class*=size-])]:size-4'
const buttonPrimary = cn(buttonBase, 'h-9 gap-1.5 px-3.5 hover:bg-[var(--primary-hover)]')
const buttonSecondary = cn(buttonBase, 'h-9 gap-1.5 px-3.5 border border-[var(--hairline)] bg-transparent text-foreground hover:bg-surface-2 hover:border-white/15 light:hover:border-black/15')

/** 读取一次性回调材料后立即清理地址栏，再进入 bearer 会话交换。 */
export function OAuthCallbackPage({ onAuthenticated }: OAuthCallbackPageProps) {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const [payload] = useState(consumeCallbackPayloadOnce)
  const [status, setStatus] = useState<CallbackStatus>(() => (
    payload.kind === 'ticket' ? 'exchanging' : payload.kind
  ))
  const started = useRef(false)

  useEffect(() => {
    if (payload.kind !== 'ticket' || started.current) return
    started.current = true
    void exchangeOAuthTicket(payload.ticket)
      .then((session) => {
        establishManagementSession(session)
        setStatus('succeeded')
        onAuthenticated()
      })
      .catch(() => {
        setStatus('failed')
      })
  }, [onAuthenticated, payload])

  const isPending = status === 'exchanging' || status === 'succeeded'
  return (
    <AuthPageShell site={siteQuery.data}>
      <Card shadow="none" className={cn(cardClass, 'mx-auto w-full max-w-[400px] bg-card/94 backdrop-blur-2xl')}>
        {/* HeroUI 头部默认横排 + items-center + p-3，补回原竖排与 p-5 基准。 */}
        <CardHeader className="flex-col items-stretch gap-1.5 p-5 gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
          <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-info">
            {isPending
              ? <LoaderCircle className="size-5 animate-spin" aria-hidden="true" />
              : <AlertCircle className="size-5 text-destructive" aria-hidden="true" />}
          </div>
          <h3 data-slot="card-title" className="text-[0.9375rem] leading-none font-semibold text-xl leading-tight">{t('auth.oauthCallback.title')}</h3>
          <p data-slot="card-description" className="text-sm text-muted-foreground leading-6">
            {t(`auth.oauthCallback.${status}`)}
          </p>
        </CardHeader>
        <CardBody className="p-5 pt-0 px-6 pb-6 sm:px-7 sm:pb-7">
          {isPending ? (
            <div className="flex min-h-11 items-center gap-2.5 rounded-lg border border-info/20 bg-info/8 px-3 text-sm text-info" role="status">
              <CheckCircle2 className="size-4" aria-hidden="true" />
              {t('auth.oauthCallback.wait')}
            </div>
          ) : (
            <div className="grid gap-2">
              {/* HeroUI Button 无 asChild，改用 as="a" 保留链接语义与原外观。 */}
              <Button data-slot="button" as="a" href="/login" className={buttonPrimary}>
                <RotateCcw aria-hidden="true" />{t('auth.oauthCallback.retry')}
              </Button>
              <Button data-slot="button" as="a" href="/models" variant="flat" className={buttonSecondary}>
                {t('auth.login.browseModels')}
              </Button>
            </div>
          )}
        </CardBody>
      </Card>
    </AuthPageShell>
  )
}

/** StrictMode 会重复调用状态初始化器，内存缓存保证票据只从地址栏读取一次。 */
function consumeCallbackPayloadOnce() {
  consumedCallbackPayload ??= consumeCallbackPayload()
  return consumedCallbackPayload
}

function consumeCallbackPayload(): CallbackPayload {
  const query = new URLSearchParams(window.location.search)
  const tickets = query.getAll('ticket')
  const errors = query.getAll('error')

  // replaceState 清掉一次性票据，避免其进入浏览器历史或后续重放。
  window.history.replaceState(null, '', '/oauth/callback')
  if (tickets.length === 1 && errors.length === 0 && ticketPattern.test(tickets[0])) {
    return { kind: 'ticket', ticket: tickets[0] }
  }
  if (tickets.length === 0 && errors.length === 1 && errors[0] === 'cancelled') {
    return { kind: 'cancelled' }
  }
  return { kind: 'failed' }
}
