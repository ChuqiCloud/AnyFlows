import type { FormEvent, ReactNode } from 'react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  CheckCircle2,
  ExternalLink,
  Link2,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  TriangleAlert,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import type {
  AdminCredential,
  AdminOAuthAuthorizationResponse,
  AdminOAuthProvider,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  adminOAuthErrorCode,
  type AdminOAuthErrorCode,
  useAdminOAuthProviders,
  useBeginAdminOAuthAuthorization,
  useCompleteAdminOAuthManualCallback,
} from './channel-api'
import {
  asAdminOAuthProvider,
  isValidOAuthCallbackUrl,
  oauthAuthorizationRemainingSeconds,
  oauthCredentialConnectionUpdated,
  oauthProviderStatus,
  type OAuthCredentialBaseline,
} from './channel-oauth-model'

type ChannelOAuthConnectionProps = {
  active: boolean
  channelId: number
  credential: AdminCredential
  onRefresh: () => Promise<unknown>
}

type ConnectionStatus = 'idle' | 'waiting' | 'connected' | 'error' | 'expired'

type ActiveAuthorization = AdminOAuthAuthorizationResponse & {
  baseline: OAuthCredentialBaseline
  expiresAt: number
}

/** 管理单条 OAuth 凭据的一次性浏览器授权与手动回调兜底。 */
export function ChannelOAuthConnection({ active, channelId, credential, onRefresh }: ChannelOAuthConnectionProps) {
  const { t } = useTranslation()
  const providersQuery = useAdminOAuthProviders(active)
  const beginMutation = useBeginAdminOAuthAuthorization()
  const manualMutation = useCompleteAdminOAuthManualCallback()
  const { reset: resetBeginMutation } = beginMutation
  const { reset: resetManualMutation } = manualMutation
  const popupRef = useRef<Window | null>(null)
  const beginAbortRef = useRef<AbortController | undefined>(undefined)
  const manualAbortRef = useRef<AbortController | undefined>(undefined)
  const boundProvider = asAdminOAuthProvider(credential.oauth_provider)
  const unsupportedBoundProvider = credential.oauth_provider !== null && boundProvider === undefined
  const providers = useMemo(() => providersQuery.data?.providers ?? [], [providersQuery.data])
  const [provider, setProvider] = useState<AdminOAuthProvider | ''>(boundProvider ?? '')
  const [status, setStatus] = useState<ConnectionStatus>('idle')
  const [authorization, setAuthorization] = useState<ActiveAuthorization>()
  const [manualVisible, setManualVisible] = useState(false)
  const [callbackUrl, setCallbackUrl] = useState('')
  const [invalidCallback, setInvalidCallback] = useState(false)
  const [popupBlocked, setPopupBlocked] = useState(false)
  const [beginError, setBeginError] = useState<AdminOAuthErrorCode>()
  const [manualError, setManualError] = useState<AdminOAuthErrorCode>()
  const [manualBusy, setManualBusy] = useState(false)
  const [now, setNow] = useState(Date.now())
  const selectedStatus = boundProvider !== undefined
    ? oauthProviderStatus(providers, credential)
    : providers.find((item) => item.provider === provider)

  useEffect(() => {
    if (boundProvider !== undefined) {
      setProvider(boundProvider)
      return
    }
    if (unsupportedBoundProvider) {
      setProvider('')
      return
    }
    setProvider((current) => providers.some((item) => item.provider === current)
      ? current
      : providers[0]?.provider ?? '')
  }, [boundProvider, providers, unsupportedBoundProvider])

  const closePopup = useCallback(() => {
    try {
      if (popupRef.current && !popupRef.current.closed) popupRef.current.close()
    } catch {
      // 跨域授权页可能拒绝状态读取；关闭失败不影响凭据状态收敛。
    }
    popupRef.current = null
  }, [])

  const resetFlow = useCallback(() => {
    beginAbortRef.current?.abort()
    manualAbortRef.current?.abort()
    beginAbortRef.current = undefined
    manualAbortRef.current = undefined
    closePopup()
    setStatus('idle')
    setAuthorization(undefined)
    setManualVisible(false)
    setCallbackUrl('')
    setInvalidCallback(false)
    setPopupBlocked(false)
    setBeginError(undefined)
    setManualError(undefined)
    setManualBusy(false)
    resetBeginMutation()
    resetManualMutation()
  }, [closePopup, resetBeginMutation, resetManualMutation])

  useEffect(() => {
    if (active) return
    resetFlow()
  }, [active, credential.id, resetFlow])

  useEffect(() => () => {
    beginAbortRef.current?.abort()
    manualAbortRef.current?.abort()
    closePopup()
  }, [closePopup])

  useEffect(() => {
    if (status !== 'waiting' || authorization === undefined) return
    let refreshing = false
    const tick = () => {
      const currentTime = Date.now()
      setNow(currentTime)
      if (currentTime >= authorization.expiresAt && !manualBusy) {
        setStatus('expired')
        setAuthorization(undefined)
        setCallbackUrl('')
        setManualError(undefined)
        closePopup()
        return
      }
      if (refreshing) return
      refreshing = true
      void onRefresh().finally(() => { refreshing = false })
    }
    tick()
    const interval = window.setInterval(tick, 2_000)
    return () => window.clearInterval(interval)
  }, [authorization, closePopup, manualBusy, onRefresh, status])

  useEffect(() => {
    if (status !== 'waiting' || authorization === undefined) return
    if (!authorization.loopback_listener_ready) {
      setManualVisible(authorization.manual_callback_supported)
      return
    }
    const timeout = window.setTimeout(() => {
      setManualVisible(authorization.manual_callback_supported)
    }, 12_000)
    return () => window.clearTimeout(timeout)
  }, [authorization, status])

  useEffect(() => {
    if (status !== 'waiting' || authorization === undefined) return
    if (!oauthCredentialConnectionUpdated(credential, authorization.provider, authorization.baseline)) return
    setStatus('connected')
    setAuthorization(undefined)
    setCallbackUrl('')
    setManualError(undefined)
    manualAbortRef.current?.abort()
    manualAbortRef.current = undefined
    setManualBusy(false)
    resetManualMutation()
    closePopup()
  }, [authorization, closePopup, credential, resetManualMutation, status])

  function openPendingPopup() {
    const width = 560
    const height = 720
    const left = Math.max(0, Math.round(window.screenX + (window.outerWidth - width) / 2))
    const top = Math.max(0, Math.round(window.screenY + (window.outerHeight - height) / 2))
    const popup = window.open(
      'about:blank',
      `anyflows-oauth-${credential.id}`,
      `popup=yes,width=${width},height=${height},left=${left},top=${top}`,
    )
    if (!popup) return null
    try {
      popup.opener = null
      popup.document.title = t('channels.credentials.oauth.preparing')
      popup.document.body.textContent = t('channels.credentials.oauth.preparing')
      popup.document.body.style.fontFamily = 'sans-serif'
      popup.document.body.style.padding = '32px'
    } catch {
      // 空白窗口初始化失败时仍可继续导航到上游授权页。
    }
    return popup
  }

  const startAuthorization = async () => {
    if (provider === '' || selectedStatus === undefined) return
    closePopup()
    const popup = openPendingPopup()
    popupRef.current = popup
    setPopupBlocked(popup === null)
    setStatus('waiting')
    setAuthorization(undefined)
    setManualVisible(false)
    setCallbackUrl('')
    setInvalidCallback(false)
    setBeginError(undefined)
    setManualError(undefined)
    beginMutation.reset()
    manualMutation.reset()
    beginAbortRef.current?.abort()
    const controller = new AbortController()
    beginAbortRef.current = controller
    try {
      const response = await beginMutation.mutateAsync({
        channelId,
        credentialId: credential.id,
        provider,
        signal: controller.signal,
      })
      beginMutation.reset()
      const startedAt = Date.now()
      setNow(startedAt)
      setAuthorization({
        ...response,
        baseline: {
          revision: credential.oauth_revision,
        },
        expiresAt: startedAt + response.expires_in_seconds * 1_000,
      })
      setManualVisible(!response.loopback_listener_ready && response.manual_callback_supported)
      if (popup && !popup.closed) {
        popup.location.replace(response.authorization_url)
      } else {
        setPopupBlocked(true)
      }
    } catch (error) {
      beginMutation.reset()
      if (controller.signal.aborted) return
      setBeginError(adminOAuthErrorCode(error))
      setStatus('error')
      closePopup()
    } finally {
      if (beginAbortRef.current === controller) beginAbortRef.current = undefined
    }
  }

  const reopenAuthorization = () => {
    if (authorization === undefined) return
    closePopup()
    const popup = openPendingPopup()
    popupRef.current = popup
    setPopupBlocked(popup === null)
    if (!popup) return
    popup.location.replace(authorization.authorization_url)
  }

  const submitManualCallback = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (authorization === undefined) return
    const valid = isValidOAuthCallbackUrl(callbackUrl, authorization.redirect_uri)
    setInvalidCallback(!valid)
    setManualError(undefined)
    if (!valid) return
    manualAbortRef.current?.abort()
    const controller = new AbortController()
    manualAbortRef.current = controller
    setManualBusy(true)
    try {
      await manualMutation.mutateAsync({
        provider: authorization.provider,
        callbackUrl,
        signal: controller.signal,
      })
      manualMutation.reset()
      manualAbortRef.current = undefined
      setManualBusy(false)
      setCallbackUrl('')
      setAuthorization(undefined)
      setStatus('connected')
      closePopup()
      void onRefresh()
    } catch (error) {
      manualMutation.reset()
      if (controller.signal.aborted) return
      const code = adminOAuthErrorCode(error)
      const callbackCanRetry = code === 'invalid_request'
        || code === 'forbidden'
        || code === 'oauth_provider_not_configured'
      if (callbackCanRetry) {
        setManualError(code)
      } else {
        setBeginError(code)
        setManualError(undefined)
        setCallbackUrl('')
        setAuthorization(undefined)
        setStatus(code === 'oauth_authorization_expired' || code === 'oauth_authorization_not_found' ? 'expired' : 'error')
        closePopup()
      }
    } finally {
      if (manualAbortRef.current === controller) {
        manualAbortRef.current = undefined
        setManualBusy(false)
      }
    }
  }

  const remainingSeconds = authorization === undefined
    ? 0
    : oauthAuthorizationRemainingSeconds(authorization.expiresAt, now)
  const providerReady = provider !== '' && selectedStatus !== undefined
  const providerUnavailable = providersQuery.isPending
    || providersQuery.isError
    || providers.length === 0
    || unsupportedBoundProvider

  return (
    <div className="mt-3 rounded-lg border border-[var(--hairline)] bg-surface-2 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2">
          <div className="grid size-7 shrink-0 place-items-center rounded-md bg-background text-muted-foreground"><Link2 className="size-3.5" aria-hidden="true" /></div>
          <div className="min-w-0">
            <p className="text-xs font-medium">{t('channels.credentials.oauth.connectionTitle')}</p>
            <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.credentials.oauth.connectionHint')}</p>
          </div>
        </div>
        {selectedStatus ? (
          <Badge className={cn('shrink-0 border-transparent', selectedStatus.loopback_listener_ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning')}>
            {t(selectedStatus.loopback_listener_ready ? 'channels.credentials.oauth.automaticCallback' : 'channels.credentials.oauth.manualCallback')}
          </Badge>
        ) : null}
      </div>

      <div className="mt-3 grid gap-1.5">
        <Label htmlFor={`credential-${credential.id}-oauth-provider`}>{t('channels.credentials.oauth.provider')}</Label>
        {providersQuery.isPending ? (
          <Skeleton className="h-8 rounded-lg" aria-label={t('channels.credentials.oauth.providersLoading')} />
        ) : providersQuery.isError ? (
          <InlineNotice tone="error" message={t('channels.credentials.oauth.providersFailed')}>
            <Button type="button" size="icon-xs" variant="ghost" aria-label={t('channels.actions.retry')} onClick={() => providersQuery.refetch()}><RefreshCw aria-hidden="true" /></Button>
          </InlineNotice>
        ) : providers.length === 0 ? (
          <InlineNotice tone="neutral" message={t('channels.credentials.oauth.providersEmpty')} />
        ) : unsupportedBoundProvider ? (
          <InlineNotice tone="error" message={t('channels.credentials.oauth.providerUnsupported', { provider: credential.oauth_provider })} />
        ) : boundProvider !== undefined && selectedStatus === undefined ? (
          <InlineNotice tone="error" message={t('channels.credentials.oauth.providerNotConfigured')}>
            <Button type="button" size="icon-xs" variant="ghost" aria-label={t('channels.actions.retry')} onClick={() => providersQuery.refetch()}><RefreshCw aria-hidden="true" /></Button>
          </InlineNotice>
        ) : boundProvider !== undefined ? (
          <div className="flex h-8 items-center gap-2 rounded-lg border border-input bg-background px-2.5">
            <Badge>{t(`channels.credentials.oauth.providers.${boundProvider}`)}</Badge>
            <span className="truncate text-[0.6875rem] text-muted-foreground">{t('channels.credentials.oauth.providerBound')}</span>
          </div>
        ) : (
          <Select
            id={`credential-${credential.id}-oauth-provider`}
            value={provider}
            disabled={status === 'waiting'}
            onChange={(event) => { setProvider(event.target.value as AdminOAuthProvider); resetFlow() }}
          >
            {providers.map((item) => <option key={item.provider} value={item.provider}>{t(`channels.credentials.oauth.providers.${item.provider}`)}</option>)}
          </Select>
        )}
      </div>

      {status === 'waiting' && authorization ? (
        <div className="mt-3 grid gap-3">
          <InlineNotice
            tone={popupBlocked ? 'error' : 'info'}
            message={t(popupBlocked ? 'channels.credentials.oauth.popupBlocked' : 'channels.credentials.oauth.waiting', { seconds: remainingSeconds })}
            icon={popupBlocked ? TriangleAlert : LoaderCircle}
            spinning={!popupBlocked}
          />
          <div className="flex flex-wrap gap-2">
            <Button type="button" size="sm" variant="secondary" disabled={manualBusy} onClick={reopenAuthorization}><ExternalLink aria-hidden="true" />{t('channels.credentials.oauth.openAuthorization')}</Button>
            {authorization.manual_callback_supported ? (
              <Button type="button" size="sm" variant="ghost" disabled={manualBusy} onClick={() => setManualVisible((current) => !current)}>{t(manualVisible ? 'channels.credentials.oauth.hideManual' : 'channels.credentials.oauth.useManual')}</Button>
            ) : null}
          </div>
          {manualVisible ? (
            <form className="grid gap-2 border-t border-[var(--hairline)] pt-3" onSubmit={submitManualCallback} noValidate>
              <div>
                <Label htmlFor={`credential-${credential.id}-oauth-callback`}>{t('channels.credentials.oauth.callbackUrl')}</Label>
                <p id={`credential-${credential.id}-oauth-callback-hint`} className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.credentials.oauth.callbackHint')}</p>
              </div>
              <Textarea
                id={`credential-${credential.id}-oauth-callback`}
                value={callbackUrl}
                rows={3}
                autoComplete="off"
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                maxLength={16 * 1024}
                disabled={manualBusy}
                className="resize-none font-mono text-xs"
                aria-describedby={`credential-${credential.id}-oauth-callback-hint`}
                aria-invalid={invalidCallback}
                onChange={(event) => {
                  setCallbackUrl(event.target.value)
                  setInvalidCallback(false)
                  setManualError(undefined)
                  manualMutation.reset()
                }}
              />
              {invalidCallback ? <p role="alert" className="text-xs text-destructive">{t('channels.credentials.oauth.callbackInvalid')}</p> : null}
              {manualError ? <p role="alert" className="text-xs text-destructive">{t(`channels.credentials.oauth.errors.${manualError}`, { defaultValue: t('channels.credentials.oauth.errors.unknown') })}</p> : null}
              <Button type="submit" size="sm" className="w-fit" disabled={manualBusy || callbackUrl.length === 0}>
                {manualBusy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CheckCircle2 aria-hidden="true" />}
                {t('channels.credentials.oauth.submitCallback')}
              </Button>
            </form>
          ) : null}
        </div>
      ) : status === 'connected' ? (
        <div className="mt-3 grid gap-2">
          <InlineNotice tone="success" icon={CheckCircle2} message={t('channels.credentials.oauth.connected')} />
          <Button type="button" size="sm" variant="secondary" className="w-fit" disabled={!providerReady || providerUnavailable} onClick={startAuthorization}><RotateCcw aria-hidden="true" />{t('channels.credentials.oauth.reauthorize')}</Button>
        </div>
      ) : status === 'error' ? (
        <div className="mt-3 grid gap-2">
          <InlineNotice tone="error" icon={TriangleAlert} message={t(`channels.credentials.oauth.errors.${beginError ?? 'unknown'}`, { defaultValue: t('channels.credentials.oauth.errors.unknown') })} />
          <Button type="button" size="sm" variant="secondary" className="w-fit" disabled={!providerReady || providerUnavailable} onClick={startAuthorization}><RotateCcw aria-hidden="true" />{t('channels.actions.retry')}</Button>
        </div>
      ) : status === 'expired' ? (
        <div className="mt-3 grid gap-2">
          <InlineNotice tone="error" icon={TriangleAlert} message={t('channels.credentials.oauth.expired')} />
          <Button type="button" size="sm" variant="secondary" className="w-fit" disabled={!providerReady || providerUnavailable} onClick={startAuthorization}><RotateCcw aria-hidden="true" />{t('channels.credentials.oauth.restart')}</Button>
        </div>
      ) : (
        <Button type="button" size="sm" className="mt-3 w-fit" disabled={!providerReady || providerUnavailable || beginMutation.isPending} onClick={startAuthorization}>
          {beginMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ExternalLink aria-hidden="true" />}
          {t(boundProvider === undefined ? 'channels.credentials.oauth.connect' : 'channels.credentials.oauth.reauthorize')}
        </Button>
      )}
    </div>
  )
}

function InlineNotice({ children, icon: Icon, message, spinning = false, tone }: {
  children?: ReactNode
  icon?: typeof Link2
  message: string
  spinning?: boolean
  tone: 'error' | 'info' | 'neutral' | 'success'
}) {
  const tones = {
    error: 'border-destructive/25 bg-destructive/8 text-destructive',
    info: 'border-info/20 bg-info/8 text-info',
    neutral: 'border-[var(--hairline)] bg-background text-muted-foreground',
    success: 'border-success/20 bg-success/8 text-success',
  } as const
  return (
    <div role={tone === 'error' ? 'alert' : tone === 'success' ? 'status' : undefined} className={cn('flex min-h-9 items-center gap-2 rounded-lg border px-3 py-2 text-xs', tones[tone])}>
      {Icon ? <Icon className={cn('size-3.5 shrink-0', spinning && 'animate-spin')} aria-hidden="true" /> : null}
      <span className="min-w-0 flex-1 leading-4">{message}</span>
      {children}
    </div>
  )
}
