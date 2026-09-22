import { useEffect } from 'react'

import type { AdminCredential, AdminOAuthProvider } from '@/lib/api/generated/types.gen'
import { oauthCredentialConnectionUpdated } from './credential-oauth-model'
import type { ActiveOAuthAuthorization, CredentialOAuthStatus } from './credential-oauth-types'

/** 在 Provider 列表变化后收敛未绑定凭据的默认选择。 */
export function useOAuthProviderSelection(options: {
  boundProvider?: AdminOAuthProvider
  provider: AdminOAuthProvider | ''
  providers: readonly { provider: AdminOAuthProvider }[]
  setProvider: (provider: AdminOAuthProvider | '') => void
  unsupportedBoundProvider: boolean
}) {
  const { boundProvider, provider, providers, setProvider, unsupportedBoundProvider } = options
  useEffect(() => {
    if (boundProvider !== undefined) {
      setProvider(boundProvider)
      return
    }
    if (unsupportedBoundProvider) {
      setProvider('')
      return
    }
    // Provider 请求尚未完成时保留恢复的选择，避免空列表把它清掉。
    if (providers.length === 0) return
    if (!providers.some((item) => item.provider === provider)) {
      setProvider(providers[0]?.provider ?? '')
    }
  }, [boundProvider, provider, providers, setProvider, unsupportedBoundProvider])
}

/** 授权等待期间定时刷新脱敏凭据，并在截止时间后立即关闭弹窗。 */
export function useOAuthAuthorizationPolling(options: {
  authorization?: ActiveOAuthAuthorization
  manualBusy: boolean
  onExpired: () => void
  onRefresh: () => Promise<unknown>
  setNow: (now: number) => void
  status: CredentialOAuthStatus
}) {
  const { authorization, manualBusy, onExpired, onRefresh, setNow, status } = options
  useEffect(() => {
    if (status !== 'waiting' || authorization === undefined) return
    let refreshing = false
    const tick = () => {
      const currentTime = Date.now()
      setNow(currentTime)
      if (currentTime >= authorization.expiresAt && !manualBusy) {
        onExpired()
        return
      }
      if (refreshing) return
      refreshing = true
      void onRefresh().finally(() => { refreshing = false })
    }
    tick()
    const interval = window.setInterval(tick, 2_000)
    return () => window.clearInterval(interval)
  }, [authorization, manualBusy, onExpired, onRefresh, setNow, status])
}

/** 自动回调不可用或等待较久时，向管理员开放手动回调兜底。 */
export function useOAuthManualFallback(options: {
  authorization?: ActiveOAuthAuthorization
  setManualVisible: (visible: boolean) => void
  status: CredentialOAuthStatus
}) {
  const { authorization, setManualVisible, status } = options
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
  }, [authorization, setManualVisible, status])
}

/** 仅在本次 Provider 的 OAuth 版本推进后判定自动授权完成。 */
export function useOAuthCredentialCompletion(options: {
  authorization?: ActiveOAuthAuthorization
  credential: AdminCredential
  onConnected: () => void
  status: CredentialOAuthStatus
}) {
  const { authorization, credential, onConnected, status } = options
  useEffect(() => {
    if (status !== 'waiting' || authorization === undefined) return
    if (oauthCredentialConnectionUpdated(
      credential,
      authorization.provider,
      authorization.baseline,
    )) onConnected()
  }, [authorization, credential, onConnected, status])
}
