import { Button, Textarea } from '@heroui/react'
import { CheckCircle2, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminCredential } from '@/lib/api/generated/types.gen'
import type { CredentialOAuthController } from './credential-oauth-controller'

/** 手动回调只在当前短期授权会话中可见，提交后立即清空完整 URL。 */
export function CredentialOAuthManualCallback({ controller, credential }: {
  controller: CredentialOAuthController
  credential: AdminCredential
}) {
  const { t } = useTranslation()
  const fieldId = `credential-${credential.id}-oauth-callback`
  const hintId = `${fieldId}-hint`
  return (
    <form className="grid gap-2 border-t border-[var(--hairline)] pt-3" onSubmit={controller.submitManualCallback} noValidate>
      <div>
        <label className="text-xs font-medium leading-none text-foreground" htmlFor={fieldId}>{t('credentials.oauth.callbackUrl')}</label>
        <p id={hintId} className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.oauth.callbackHint')}</p>
      </div>
      <Textarea
        autoCapitalize="none"
        autoComplete="off"
        autoCorrect="off"
        classNames={{ input: 'resize-none font-mono text-xs' }}
        id={fieldId}
        isDisabled={controller.manualBusy}
        isInvalid={controller.invalidCallback}
        maxLength={16 * 1024}
        minRows={3}
        size="sm"
        spellCheck="false"
        value={controller.callbackUrl}
        aria-describedby={hintId}
        onChange={(event) => controller.setCallback(event.target.value)}
      />
      {controller.invalidCallback ? <p role="alert" className="text-xs text-destructive">{t('credentials.oauth.callbackInvalid')}</p> : null}
      {controller.manualError ? <p role="alert" className="text-xs text-destructive">{t(`credentials.oauth.errors.${controller.manualError}`, { defaultValue: t('credentials.oauth.errors.unknown') })}</p> : null}
      <Button type="submit" className="w-fit" color="primary" isDisabled={controller.manualBusy || controller.callbackUrl.length === 0} size="sm">
        {controller.manualBusy ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <CheckCircle2 className="size-3.5" aria-hidden="true" />}
        {t('credentials.oauth.submitCallback')}
      </Button>
    </form>
  )
}
