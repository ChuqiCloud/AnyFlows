import { AlertCircle, CheckCircle2, LoaderCircle, RotateCcw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { parseOAuthCallbackPayload, type OAuthCallbackPayload } from '@/features/auth/oauth-callback-payload'
import { extensionLoginMethods } from '@/extensions/registry'
import { exchangeOAuthTicket } from '@/features/auth/oauth-login-api'
import { establishManagementSession } from '@/features/auth/session-query'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'

type OAuthCallbackPageProps = {
  onAuthenticated: () => void
}

type CallbackStatus = 'exchanging' | 'succeeded' | 'cancelled' | 'failed'

let consumedCallbackPayload: OAuthCallbackPayload | undefined

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
    const method = extensionLoginMethods.find((entry) => `${entry.namespace}:${entry.id}` === payload.method)
    const exchange = payload.method ? method?.exchangeTicket : exchangeOAuthTicket
    const result = exchange ? exchange(payload.ticket) : Promise.reject(new Error('Unknown login method'))
    void result
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
      <Card elevation="overlay" className="mx-auto w-full max-w-[400px] bg-card/94 backdrop-blur-2xl">
        <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
          <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-info">
            {isPending
              ? <LoaderCircle className="size-5 animate-spin" aria-hidden="true" />
              : <AlertCircle className="size-5 text-destructive" aria-hidden="true" />}
          </div>
          <CardTitle className="text-xl leading-tight">{t('auth.oauthCallback.title')}</CardTitle>
          <CardDescription className="leading-6">
            {t(`auth.oauthCallback.${status}`)}
          </CardDescription>
        </CardHeader>
        <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
          {isPending ? (
            <div className="flex min-h-11 items-center gap-2.5 rounded-lg border border-info/20 bg-info/8 px-3 text-sm text-info" role="status">
              <CheckCircle2 className="size-4" aria-hidden="true" />
              {t('auth.oauthCallback.wait')}
            </div>
          ) : (
            <div className="grid gap-2">
              <Button asChild>
                <a href="#/login"><RotateCcw aria-hidden="true" />{t('auth.oauthCallback.retry')}</a>
              </Button>
              <Button variant="secondary" asChild>
                <a href="#/models">{t('auth.login.browseModels')}</a>
              </Button>
            </div>
          )}
        </CardContent>
      </Card>
    </AuthPageShell>
  )
}

/** StrictMode 会重复调用状态初始化器，内存缓存保证票据只从地址栏读取一次。 */
function consumeCallbackPayloadOnce() {
  consumedCallbackPayload ??= consumeCallbackPayload()
  return consumedCallbackPayload
}

function consumeCallbackPayload(): OAuthCallbackPayload {
  const payload = parseOAuthCallbackPayload(window.location.search, window.location.hash)

  // replaceState 不触发 hashchange，避免原始票据再次进入路由状态或浏览器历史。
  window.history.replaceState(null, '', `${window.location.pathname}#/oauth/callback`)
  return payload
}
