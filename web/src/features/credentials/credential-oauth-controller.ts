import type { FormEvent } from 'react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import type { AdminCredential, AdminOAuthProvider } from '@/lib/api/generated/types.gen'
import {
  adminOAuthErrorCode,
  type AdminOAuthErrorCode,
  useAdminOAuthProviders,
  useBeginAdminOAuthAuthorization,
  useCompleteAdminOAuthManualCallback,
} from './credential-api'
import {
  useOAuthAuthorizationPolling,
  useOAuthCredentialCompletion,
  useOAuthManualFallback,
  useOAuthProviderSelection,
} from './credential-oauth-effects'
import {
  asAdminOAuthProvider,
  clearOAuthWizardState,
  isValidOAuthCallbackUrl,
  oauthAuthorizationRemainingSeconds,
  readOAuthWizardState,
  writeOAuthWizardState,
} from './credential-oauth-model'
import { useCredentialOAuthPopup } from './credential-oauth-popup'
import type { ActiveOAuthAuthorization, CredentialOAuthStatus } from './credential-oauth-types'

type CredentialOAuthControllerOptions = {
  active: boolean
  channelType: string
  channelId: number
  credential: AdminCredential
  onRefresh: () => Promise<unknown>
  preparingLabel: string
}

/** 管理单条 OAuth 凭据的一次性授权状态，不向视图暴露敏感响应。 */
export function useCredentialOAuthController(options: CredentialOAuthControllerOptions) {
  const providersQuery = useAdminOAuthProviders(options.active)
  const beginMutation = useBeginAdminOAuthAuthorization()
  const manualMutation = useCompleteAdminOAuthManualCallback()
  const { reset: resetBeginMutation } = beginMutation
  const { reset: resetManualMutation } = manualMutation
  const beginAbortRef = useRef<AbortController | undefined>(undefined)
  const manualAbortRef = useRef<AbortController | undefined>(undefined)
  const boundProvider = asAdminOAuthProvider(options.credential.oauth_provider) ?? (options.channelType === 'openai' ? 'codex' : undefined)
  const unsupportedBoundProvider = options.channelType !== 'openai' && options.credential.oauth_provider !== null && boundProvider === undefined
  const providers = useMemo(() => providersQuery.data?.providers ?? [], [providersQuery.data])
  const [restoredState] = useState(() => readOAuthWizardState(options.channelId, options.credential.id))
  const restoredProvider = restoredState !== undefined
    && (boundProvider === undefined || restoredState.provider === boundProvider)
    ? restoredState.provider
    : undefined
  const [provider, setProvider] = useState<AdminOAuthProvider | ''>(restoredProvider ?? boundProvider ?? '')
  const [status, setStatus] = useState<CredentialOAuthStatus>(() => {
    if (restoredProvider === undefined || restoredState === undefined) return 'idle'
    return restoredState.phase === 'waiting' ? 'interrupted' : restoredState.phase
  })
  const [authorization, setAuthorization] = useState<ActiveOAuthAuthorization>()
  const [manualVisible, setManualVisible] = useState(false)
  const [callbackUrl, setCallbackUrl] = useState('')
  const [invalidCallback, setInvalidCallback] = useState(false)
  const [popupBlocked, setPopupBlocked] = useState(false)
  const [beginError, setBeginError] = useState<AdminOAuthErrorCode>()
  const [manualError, setManualError] = useState<AdminOAuthErrorCode>()
  const [manualBusy, setManualBusy] = useState(false)
  const [now, setNow] = useState(Date.now())
  const { closePopup, openPendingPopup } = useCredentialOAuthPopup({
    credentialId: options.credential.id,
    preparingLabel: options.preparingLabel,
  })
  const selectedStatus = boundProvider !== undefined
    ? providers.find((item) => item.provider === boundProvider)
    : providers.find((item) => item.provider === provider)

  useEffect(() => {
    if (!options.active || status === 'idle' || status === 'connected' || provider === '') {
      clearOAuthWizardState(options.channelId, options.credential.id)
      return
    }
    writeOAuthWizardState({
      channelId: options.channelId,
      credentialId: options.credential.id,
      provider,
      phase: status === 'expired' ? 'expired' : status === 'error' ? 'error' : 'waiting',
    })
  }, [options.active, options.channelId, options.credential.id, provider, status])

  const resetFlow = useCallback(() => {
    beginAbortRef.current?.abort()
    manualAbortRef.current?.abort()
    beginAbortRef.current = undefined
    manualAbortRef.current = undefined
    closePopup()
    clearOAuthWizardState(options.channelId, options.credential.id)
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
  }, [closePopup, options.channelId, options.credential.id, resetBeginMutation, resetManualMutation])

  const expireAuthorization = useCallback(() => {
    setStatus('expired')
    setAuthorization(undefined)
    setCallbackUrl('')
    setManualError(undefined)
    closePopup()
  }, [closePopup])

  const completeAuthorization = useCallback(() => {
    setStatus('connected')
    setAuthorization(undefined)
    setCallbackUrl('')
    setManualError(undefined)
    manualAbortRef.current?.abort()
    manualAbortRef.current = undefined
    setManualBusy(false)
    resetManualMutation()
    closePopup()
  }, [closePopup, resetManualMutation])

  useOAuthProviderSelection({ boundProvider, provider, providers, setProvider, unsupportedBoundProvider })
  useOAuthAuthorizationPolling({ authorization, manualBusy, onExpired: expireAuthorization, onRefresh: options.onRefresh, setNow, status })
  useOAuthManualFallback({ authorization, setManualVisible, status })
  useOAuthCredentialCompletion({ authorization, credential: options.credential, onConnected: completeAuthorization, status })

  useEffect(() => {
    if (!options.active) resetFlow()
  }, [options.active, options.credential.id, resetFlow])
  useEffect(() => () => {
    beginAbortRef.current?.abort()
    manualAbortRef.current?.abort()
  }, [])

  const startAuthorization = async () => {
    if (provider === '' || selectedStatus === undefined) return
    closePopup()
    const popup = openPendingPopup()
    setPopupBlocked(popup === null)
    setStatus('waiting')
    setAuthorization(undefined)
    setManualVisible(false)
    setCallbackUrl('')
    setInvalidCallback(false)
    setBeginError(undefined)
    setManualError(undefined)
    resetBeginMutation()
    resetManualMutation()
    beginAbortRef.current?.abort()
    const controller = new AbortController()
    beginAbortRef.current = controller
    try {
      const response = await beginMutation.mutateAsync({
        channelId: options.channelId,
        credentialId: options.credential.id,
        provider,
        signal: controller.signal,
      })
      resetBeginMutation()
      const startedAt = Date.now()
      setNow(startedAt)
      setAuthorization({
        ...response,
        baseline: { revision: options.credential.oauth_revision },
        expiresAt: startedAt + response.expires_in_seconds * 1_000,
      })
      setManualVisible(!response.loopback_listener_ready && response.manual_callback_supported)
      if (popup && !popup.closed) popup.location.replace(response.authorization_url)
      else setPopupBlocked(true)
    } catch (error) {
      resetBeginMutation()
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
    setPopupBlocked(popup === null)
    if (popup) popup.location.replace(authorization.authorization_url)
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
      await manualMutation.mutateAsync({ provider: authorization.provider, callbackUrl, signal: controller.signal })
      resetManualMutation()
      completeAuthorization()
      void options.onRefresh()
    } catch (error) {
      resetManualMutation()
      if (controller.signal.aborted) return
      const code = adminOAuthErrorCode(error)
      handleManualError(code, closePopup, setBeginError, setManualError, setStatus)
      if (code !== 'invalid_request' && code !== 'forbidden' && code !== 'oauth_provider_not_configured') {
        setCallbackUrl('')
        setAuthorization(undefined)
      }
    } finally {
      if (manualAbortRef.current === controller) {
        manualAbortRef.current = undefined
        setManualBusy(false)
      }
    }
  }

  return {
    authorization, beginError, beginPending: beginMutation.isPending, boundProvider, callbackUrl,
    invalidCallback, manualBusy, manualError, manualVisible, popupBlocked, provider,
    providerReady: provider !== '' && selectedStatus !== undefined,
    providerUnavailable: providersQuery.isPending || providersQuery.isError || providers.length === 0 || unsupportedBoundProvider,
    providers, providersQuery, remainingSeconds: authorization ? oauthAuthorizationRemainingSeconds(authorization.expiresAt, now) : 0,
    selectedStatus, status, unsupportedBoundProvider,
    reopenAuthorization, resetFlow, startAuthorization, submitManualCallback,
    retryProviders: () => providersQuery.refetch(),
    selectProvider: (nextProvider: AdminOAuthProvider) => { setProvider(nextProvider); resetFlow() },
    setCallback: (value: string) => { setCallbackUrl(value); setInvalidCallback(false); setManualError(undefined); resetManualMutation() },
    toggleManual: () => setManualVisible((current) => !current),
  }
}

export type CredentialOAuthController = ReturnType<typeof useCredentialOAuthController>

function handleManualError(
  code: AdminOAuthErrorCode,
  closePopup: () => void,
  setBeginError: (code?: AdminOAuthErrorCode) => void,
  setManualError: (code?: AdminOAuthErrorCode) => void,
  setStatus: (status: CredentialOAuthStatus) => void,
) {
  if (code === 'invalid_request' || code === 'forbidden' || code === 'oauth_provider_not_configured') {
    setManualError(code)
    return
  }
  setBeginError(code)
  setManualError(undefined)
  setStatus(code === 'oauth_authorization_expired' || code === 'oauth_authorization_not_found' ? 'expired' : 'error')
  closePopup()
}
