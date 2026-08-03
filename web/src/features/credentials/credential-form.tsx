import { useEffect, useMemo, useRef } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Check, LoaderCircle } from 'lucide-react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import { useCreateAdminCredential, useUpdateAdminCredential } from './credential-api'
import { CredentialSection } from './credential-form-field'
import {
  buildCredentialFormSchema,
  createCredentialRequest,
  credentialFormValues,
  updateCredentialRequest,
  type CredentialFormValues,
} from './credential-form-model'
import { CredentialKindPicker } from './credential-kind-picker'
import { credentialKindsForChannel } from './credential-model'
import { CredentialOAuthFields } from './credential-oauth-fields'
import { CredentialScheduleFields } from './credential-schedule-fields'
import { CredentialSecretFields } from './credential-secret-fields'

type CredentialFormProps = {
  channel: AdminChannel
  credential?: AdminCredential
  credentials: readonly AdminCredential[]
  active: boolean
  onSaved: (credential: AdminCredential) => void
}

/** 统一创建与编辑流程，编辑默认保留密文，显式轮换时才读取敏感输入。 */
export function CredentialForm({ channel, credential, credentials, active, onSaved }: CredentialFormProps) {
  const { t } = useTranslation()
  const creating = credential === undefined
  const allowedKinds = credentialKindsForChannel(channel.type)
  const initialKind = allowedKinds[0] ?? 'api_key'
  const controllerRef = useRef<AbortController>()
  const createMutation = useCreateAdminCredential()
  const updateMutation = useUpdateAdminCredential()
  const { reset: resetCreateMutation } = createMutation
  const { reset: resetUpdateMutation } = updateMutation
  const schema = useMemo(() => buildCredentialFormSchema({
    channelType: channel.type,
    creating,
    currentId: credential?.id,
    messages: {
      incompatibleKind: t('credentials.validation.incompatibleKind'),
      secret: t('credentials.validation.secret'),
      oauthProvider: t('credentials.validation.oauthProvider'),
      serviceAccountEmail: t('credentials.validation.serviceAccountEmail'),
      privateKey: t('credentials.validation.privateKey'),
      status: t('credentials.validation.status'),
      integer: t('credentials.validation.integer'),
      nonNegativeInteger: t('credentials.validation.nonNegativeInteger'),
      multiplier: t('credentials.validation.multiplier'),
      parent: t('credentials.validation.parent'),
      optionalText: t('credentials.validation.optionalText'),
    },
  }), [channel.type, creating, credential?.id, t])
  const form = useForm<CredentialFormValues>({
    defaultValues: credentialFormValues(credential, initialKind),
    resolver: zodResolver(schema),
  })
  const busy = createMutation.isPending || updateMutation.isPending
  const kind = form.watch('kind')

  useEffect(() => {
    controllerRef.current?.abort()
    form.reset(credentialFormValues(credential, initialKind))
    resetCreateMutation()
    resetUpdateMutation()
  }, [channel.id, credential, form, initialKind, resetCreateMutation, resetUpdateMutation])

  useEffect(() => {
    if (active) return
    controllerRef.current?.abort()
    form.reset(credentialFormValues(credential, initialKind))
  }, [active, credential, form, initialKind])

  useEffect(() => () => controllerRef.current?.abort(), [])

  const submit = form.handleSubmit(async (values) => {
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    try {
      const saved = creating
        ? await createMutation.mutateAsync({ channelId: channel.id, body: createCredentialRequest(values), signal: controller.signal })
        : await updateMutation.mutateAsync({ channelId: channel.id, credentialId: credential.id, body: updateCredentialRequest(values, credential), signal: controller.signal })
      if (saved) onSaved(saved)
    } catch {
      // 保留非敏感调度输入和本次轮换明文，错误只呈现固定分类供管理员重试。
    } finally {
      if (controllerRef.current === controller) controllerRef.current = undefined
    }
  })

  const selectKind = (nextKind: CredentialFormValues['kind']) => {
    form.setValue('kind', nextKind, { shouldDirty: true, shouldValidate: true })
    for (const field of ['apiKey', 'accessToken', 'accessKeyId', 'secretAccessKey', 'sessionToken', 'clientEmail', 'privateKeyId', 'privateKey', 'oauthProvider', 'oauthAccountKey', 'oauthProjectId'] as const) {
      form.setValue(field, '', { shouldDirty: false })
    }
    form.clearErrors()
  }

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="min-h-0 flex-1 overflow-y-auto">
        <CredentialSection title={t('credentials.form.identityTitle')} description={t('credentials.form.identityDescription')}>
          {creating ? <CredentialKindPicker value={kind} kinds={allowedKinds} disabled={busy} onChange={selectKind} /> : (
            <div className="flex items-center gap-2"><Badge>{t(`credentials.kind.${kind}`)}</Badge><span className="text-[0.6875rem] text-muted-foreground">#{credential.id}</span></div>
          )}
          <CredentialSecretFields form={form} creating={creating} disabled={busy} />
        </CredentialSection>

        {kind === 'oauth' ? (
          <CredentialSection title={t('credentials.oauth.title')} description={t('credentials.oauth.description')}>
            <CredentialOAuthFields form={form} active={active} disabled={busy} />
          </CredentialSection>
        ) : null}

        <CredentialSection title={t('credentials.schedule.title')} description={t('credentials.schedule.description')}>
          <CredentialScheduleFields form={form} credentials={credentials} credential={credential} disabled={busy} />
        </CredentialSection>
      </div>

      <div className="flex min-h-14 items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-5 py-3">
        <div className="text-xs">
          {createMutation.isError || updateMutation.isError ? <span role="alert" className="text-destructive">{t('credentials.errors.save')}</span> : form.formState.isDirty ? <span className="text-muted-foreground">{t('credentials.state.unsaved')}</span> : <span className="text-muted-foreground">{t('credentials.state.synced')}</span>}
        </div>
        <Button type="submit" disabled={busy || (!creating && !form.formState.isDirty)}>
          {busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Check aria-hidden="true" />}
          {t(creating ? 'credentials.actions.create' : 'credentials.actions.save')}
        </Button>
      </div>
    </form>
  )
}
