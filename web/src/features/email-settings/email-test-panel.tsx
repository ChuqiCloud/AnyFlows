import { CheckCircle2, LoaderCircle, MailCheck, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { AdminEmailSettings } from '@/lib/api/generated/types.gen'
import { isEmailSettingsError, useSendAdminEmailTest } from './email-settings-api'
import { isValidEmail } from './email-settings-form-model'

type EmailTestPanelProps = {
  settings: AdminEmailSettings
  settingsDirty: boolean
}

/** 测试收件人只保留在组件内存，并且成功或失败响应都不回显地址。 */
export function EmailTestPanel({ settings, settingsDirty }: EmailTestPanelProps) {
  const { t } = useTranslation()
  const mutation = useSendAdminEmailTest()
  const [recipient, setRecipient] = useState('')
  const [validationError, setValidationError] = useState(false)
  const ready = settings.delivery_ready && !settingsDirty

  const send = async () => {
    if (!isValidEmail(recipient)) {
      setValidationError(true)
      return
    }
    setValidationError(false)
    try {
      await mutation.mutateAsync({ recipient })
    } catch {
      // 只按固定错误码展示通用文案，底层 SMTP 响应与收件人不进入界面。
    }
  }

  const errorKey = isEmailSettingsError(mutation.error, 'email_not_configured')
    ? 'emailSettings.test.errors.notConfigured'
    : 'emailSettings.test.errors.delivery'

  return (
    <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/55 shadow-[var(--shadow-subtle)]">
      <div className="flex items-start justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
        <div className="flex min-w-0 items-start gap-2.5">
          <div className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-info">
            <MailCheck className="size-4" aria-hidden="true" />
          </div>
          <div>
            <h3 className="text-sm font-semibold">{t('emailSettings.test.title')}</h3>
            <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">
              {t('emailSettings.test.description')}
            </p>
          </div>
        </div>
        <Badge className={ready ? 'border-success/25 bg-success/10 text-success' : undefined}>
          {t(ready ? 'emailSettings.test.ready' : 'emailSettings.test.blocked')}
        </Badge>
      </div>

      <div className="grid gap-3 p-4">
        <div className="grid gap-1.5">
          <Label htmlFor="email-settings-test-recipient">{t('emailSettings.test.recipient')}</Label>
          <Input
            id="email-settings-test-recipient"
            type="email"
            autoCapitalize="none"
            autoComplete="off"
            spellCheck={false}
            value={recipient}
            aria-invalid={validationError}
            onChange={(event) => {
              setRecipient(event.target.value)
              setValidationError(false)
              mutation.reset()
            }}
          />
          <p className={validationError ? 'text-xs text-destructive' : 'text-[0.6875rem] leading-4 text-muted-foreground'}>
            {t(validationError
              ? 'emailSettings.test.errors.recipient'
              : settingsDirty
                ? 'emailSettings.test.saveFirst'
                : settings.delivery_ready
                  ? 'emailSettings.test.recipientHint'
                  : 'emailSettings.test.configureFirst')}
          </p>
        </div>

        <Button
          type="button"
          variant="secondary"
          className="w-full"
          disabled={!ready || mutation.isPending}
          onClick={() => void send()}
        >
          {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mutation.isPending ? 'emailSettings.test.sending' : 'emailSettings.test.send')}
        </Button>

        <div className="min-h-5 text-xs" aria-live="polite">
          {mutation.isSuccess ? (
            <span className="inline-flex items-center gap-1.5 text-success">
              <CheckCircle2 className="size-3.5" aria-hidden="true" />
              {t('emailSettings.test.success')}
            </span>
          ) : mutation.isError ? (
            <span role="alert" className="inline-flex items-center gap-1.5 text-destructive">
              <TriangleAlert className="size-3.5" aria-hidden="true" />
              {t(errorKey)}
            </span>
          ) : null}
        </div>
      </div>
    </section>
  )
}
