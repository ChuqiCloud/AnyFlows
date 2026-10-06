import { CheckCircle2, ExternalLink, Info, LoaderCircle, RotateCcw, Trash2, TriangleAlert, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@heroui/react'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import type { CredentialOAuthController } from './credential-oauth-controller'
import { CredentialOAuthManualCallback } from './credential-oauth-manual-callback'
import { CredentialOAuthNotice } from './credential-oauth-notice'

/** 将授权状态映射为单一操作区，避免同时出现冲突的连接动作。 */
export function CredentialOAuthStatusView({ controller, credential, onDelete }: {
  controller: CredentialOAuthController
  credential: AdminCredential
  onDelete: () => void
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
          <Button type="button" size="sm" variant="bordered" isDisabled={controller.manualBusy} onClick={controller.reopenAuthorization}><ExternalLink className="size-3.5" aria-hidden="true" />{t('credentials.oauth.openAuthorization')}</Button>
          {controller.authorization.manual_callback_supported ? (
            <Button type="button" size="sm" variant="light" isDisabled={controller.manualBusy} onClick={controller.toggleManual}>{t(controller.manualVisible ? 'credentials.oauth.hideManual' : 'credentials.oauth.useManual')}</Button>
          ) : null}
          <Button type="button" size="sm" variant="light" isDisabled={controller.manualBusy} onClick={controller.resetFlow}><X className="size-3.5" aria-hidden="true" />{t('credentials.oauth.cancel')}</Button>
        </div>
        {controller.manualVisible ? <CredentialOAuthManualCallback controller={controller} credential={credential} /> : null}
      </div>
    )
  }
  if (controller.status === 'interrupted') {
    return <OAuthTerminalState controller={controller} tone="info" message={t('credentials.oauth.interrupted')} action={t('credentials.oauth.restart')} onDelete={credential.oauth_token_pending ? onDelete : undefined} deleteLabel={t('credentials.oauth.clearPending')} />
  }
  if (controller.status === 'connected') {
    return <OAuthTerminalState controller={controller} tone="success" message={t('credentials.oauth.connected')} action={t('credentials.oauth.reauthorize')} />
  }
  if (controller.status === 'error') {
    return <OAuthTerminalState controller={controller} tone="error" message={t(`credentials.oauth.errors.${controller.beginError ?? 'unknown'}`, { defaultValue: t('credentials.oauth.errors.unknown') })} action={t('credentials.actions.retry')} onDelete={credential.oauth_token_pending ? onDelete : undefined} deleteLabel={t('credentials.oauth.clearPending')} />
  }
  if (controller.status === 'expired') {
    return <OAuthTerminalState controller={controller} tone="error" message={t('credentials.oauth.expired')} action={t('credentials.oauth.restart')} onDelete={credential.oauth_token_pending ? onDelete : undefined} deleteLabel={t('credentials.oauth.clearPending')} />
  }
  return (
    <div className="mt-3 flex flex-wrap gap-2">
      <Button type="button" size="sm" color="primary" isDisabled={!controller.providerReady || controller.providerUnavailable || controller.beginPending} onClick={controller.startAuthorization}>
        {controller.beginPending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <ExternalLink className="size-3.5" aria-hidden="true" />}
        {t(credential.oauth_token_pending
          ? 'credentials.oauth.connectPending'
          : controller.boundProvider === undefined ? 'credentials.oauth.connect' : 'credentials.oauth.reauthorize')}
      </Button>
      {credential.oauth_token_pending ? <Button type="button" size="sm" variant="light" className="text-destructive hover:text-destructive" onClick={onDelete}><Trash2 className="size-3.5" aria-hidden="true" />{t('credentials.oauth.clearPending')}</Button> : null}
    </div>
  )
}

function OAuthTerminalState({ action, controller, deleteLabel, message, onDelete, tone }: {
  action: string
  controller: CredentialOAuthController
  message: string
  tone: 'error' | 'info' | 'success'
  onDelete?: () => void
  deleteLabel?: string
}) {
  return (
    <div className="mt-3 grid gap-2">
      <CredentialOAuthNotice tone={tone} icon={tone === 'success' ? CheckCircle2 : tone === 'info' ? Info : TriangleAlert} message={message} />
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" variant="bordered" isDisabled={!controller.providerReady || controller.providerUnavailable} onClick={controller.startAuthorization}><RotateCcw className="size-3.5" aria-hidden="true" />{action}</Button>
        {onDelete && deleteLabel ? <Button type="button" size="sm" variant="light" className="text-destructive hover:text-destructive" onClick={onDelete}><Trash2 className="size-3.5" aria-hidden="true" />{deleteLabel}</Button> : null}
      </div>
    </div>
  )
}
