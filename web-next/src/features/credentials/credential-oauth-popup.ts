import { useCallback, useEffect, useRef } from 'react'

type CredentialOAuthPopupOptions = {
  credentialId: number
  preparingLabel: string
}

/** 隔离跨域授权弹窗的创建、复用与销毁，避免授权状态机直接持有浏览器细节。 */
export function useCredentialOAuthPopup({ credentialId, preparingLabel }: CredentialOAuthPopupOptions) {
  const popupRef = useRef<Window | null>(null)

  const closePopup = useCallback(() => {
    try {
      if (popupRef.current && !popupRef.current.closed) popupRef.current.close()
    } catch {
      // 跨域授权页可能拒绝状态读取；关闭失败不影响凭据状态收敛。
    }
    popupRef.current = null
  }, [])

  const openPendingPopup = useCallback(() => {
    const width = 560
    const height = 720
    const left = Math.max(0, Math.round(window.screenX + (window.outerWidth - width) / 2))
    const top = Math.max(0, Math.round(window.screenY + (window.outerHeight - height) / 2))
    const popup = window.open(
      'about:blank',
      `anyflows-oauth-${credentialId}`,
      `popup=yes,width=${width},height=${height},left=${left},top=${top}`,
    )
    popupRef.current = popup
    if (!popup) return null
    try {
      popup.opener = null
      popup.document.title = preparingLabel
      popup.document.body.textContent = preparingLabel
      popup.document.body.style.fontFamily = 'sans-serif'
      popup.document.body.style.padding = '32px'
    } catch {
      // 空白窗口初始化失败时仍可继续导航到上游授权页。
    }
    return popup
  }, [credentialId, preparingLabel])

  useEffect(() => closePopup, [closePopup])

  return { closePopup, openPendingPopup }
}
