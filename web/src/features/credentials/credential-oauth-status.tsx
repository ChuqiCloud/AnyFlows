import { CheckCircle2, ExternalLink, LoaderCircle, RotateCcw, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import type { CredentialOAuthController } from './credential-oauth-controller'
import { CredentialOAuthManualCallback } from './credential-oauth-manual-callback'
import { CredentialOAuthNotice } from './credential-oauth-notice'

/** 将授权状态映射为单一操作区，避免同时出现冲突的连接动作。 */
export function CredentialOAuthStatusView({ controller, credential }: {
  controller: CredentialOAuthController
  credential: AdminCredential
}) {
  const { t } = useTranslation()
  if (controller.status === 'waiting' && controller.authorization) {
    return (
      <div className="mt-3 grid gap-3">
        <CredentialOAuthNotice
          tone={controller.popupBlocked ? 'error' : 'info'}
          message={t(controller.popupBlocked ? 'credentials.oauth.popupBlocked' : 'credentials.oauth.waiting', { seconds: controller.remainingSeconds })}
          icon={controller.popupBlocked ? TriangleAlert : LoaderCircle}
          spinning={!controller.popupBlocked}
        />
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="secondary" disabled={controller.manualBusy} onClick={controller.reopenAuthorization}><ExternalLink aria-hidden="true" />{t('credentials.oauth.openAuthorization')}</Button>
          {controller.authorization.manual_callback_supported ? (
            <Button type="button" size="sm" variant="ghost" disabled={controller.manualBusy} onClick={controller.toggleManual}>{t(controller.manualVisible ? 'credentials.oauth.hideManual' : 'credentials.oauth.useManual')}</Button>
          ) : null}
        </div>
        {controller.manualVisible ? <CredentialOAuthManualCallback controller={controller} credential={credential} /> : null}
      </div>
    )
  }
  if (controller.status === 'connected') {
    return <OAuthTerminalState controller={controller} tone="success" message={t('credentials.oauth.connected')} action={t('credentials.oauth.reauthorize')} />
  }
  if (controller.status === 'error') {
    return <OAuthTerminalState controller={controller} tone="error" message={t(`credentials.oauth.errors.${controller.beginError ?? 'unknown'}`, { defaultValue: t('credentials.oauth.errors.unknown') })} action={t('credentials.actions.retry')} />
  }
  if (controller.status === 'expired') {
    return <OAuthTerminalState controller={controller} tone="error" message={t('credentials.oauth.expired')} action={t('credentials.oauth.restart')} />
  }
  return (
    <Button type="button" size="sm" className="mt-3 w-fit" disabled={!controller.providerReady || controller.providerUnavailable || controller.beginPending} onClick={controller.startAuthorization}>
      {controller.beginPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ExternalLink aria-hidden="true" />}
      {t(credential.oauth_token_pending
        ? 'credentials.oauth.connectPending'
        : controller.boundProvider === undefined ? 'credentials.oauth.connect' : 'credentials.oauth.reauthorize')}
    </Button>
  )
}

function OAuthTerminalState({ action, controller, message, tone }: {
  action: string
  controller: CredentialOAuthController
  message: string
  tone: 'error' | 'success'
}) {
  return (
    <div className="mt-3 grid gap-2">
      <CredentialOAuthNotice tone={tone} icon={tone === 'success' ? CheckCircle2 : TriangleAlert} message={message} />
      <Button type="button" size="sm" variant="secondary" className="w-fit" disabled={!controller.providerReady || controller.providerUnavailable} onClick={controller.startAuthorization}><RotateCcw aria-hidden="true" />{action}</Button>
    </div>
  )
}
