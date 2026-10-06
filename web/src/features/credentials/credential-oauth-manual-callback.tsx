import { CheckCircle2, LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
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
        <Label htmlFor={fieldId}>{t('credentials.oauth.callbackUrl')}</Label>
        <p id={hintId} className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.oauth.callbackHint')}</p>
      </div>
      <Textarea
        id={fieldId}
        value={controller.callbackUrl}
        rows={3}
        autoComplete="off"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        maxLength={16 * 1024}
        disabled={controller.manualBusy}
        className="resize-none font-mono text-xs"
        aria-describedby={hintId}
        aria-invalid={controller.invalidCallback}
        onChange={(event) => controller.setCallback(event.target.value)}
      />
      {controller.invalidCallback ? <p role="alert" className="text-xs text-destructive">{t('credentials.oauth.callbackInvalid')}</p> : null}
      {controller.manualError ? <p role="alert" className="text-xs text-destructive">{t(`credentials.oauth.errors.${controller.manualError}`, { defaultValue: t('credentials.oauth.errors.unknown') })}</p> : null}
      <Button type="submit" size="sm" className="w-fit" disabled={controller.manualBusy || controller.callbackUrl.length === 0}>
        {controller.manualBusy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CheckCircle2 aria-hidden="true" />}
        {t('credentials.oauth.submitCallback')}
      </Button>
    </form>
  )
}
