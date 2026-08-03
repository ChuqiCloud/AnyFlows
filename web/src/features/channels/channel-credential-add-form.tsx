import type { FormEvent } from 'react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { Eye, EyeOff, KeyRound, Link2, LoaderCircle, Plus, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminChannel, AdminOAuthProvider } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useAdminOAuthProviders, useCreateAdminCredential } from './channel-api'
import {
  createApiKeyRequest,
  createOAuthCredentialRequest,
  isValidCredentialSchedule,
  isValidCredentialSecret,
} from './channel-credential-model'

type ChannelCredentialAddFormProps = {
  channel?: AdminChannel
  open: boolean
}

type CredentialFormKind = 'api_key' | 'oauth'

/** 使用类型化表单添加 API Key 或带现有 access token 的 OAuth 凭据。 */
export function ChannelCredentialAddForm({ channel, open }: ChannelCredentialAddFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminCredential()
  const { reset: resetCreateMutation } = createMutation
  const createAbortRef = useRef<AbortController | undefined>(undefined)
  const [kind, setKind] = useState<CredentialFormKind>('api_key')
  const [provider, setProvider] = useState<AdminOAuthProvider | ''>('')
  const [secret, setSecret] = useState('')
  const [priority, setPriority] = useState('0')
  const [weight, setWeight] = useState('10')
  const [showSecret, setShowSecret] = useState(false)
  const [invalidSecret, setInvalidSecret] = useState(false)
  const [invalidSchedule, setInvalidSchedule] = useState(false)
  const [createFailed, setCreateFailed] = useState(false)
  const [createBusy, setCreateBusy] = useState(false)
  const providersQuery = useAdminOAuthProviders(open && kind === 'oauth')
  const providers = useMemo(() => providersQuery.data?.providers ?? [], [providersQuery.data])
  const selectedProvider = providers.find((item) => item.provider === provider)

  useEffect(() => {
    createAbortRef.current?.abort()
    setKind('api_key')
    setProvider('')
    resetFields()
    resetCreateMutation()
  }, [channel?.id, resetCreateMutation])

  useEffect(() => {
    if (open) return
    createAbortRef.current?.abort()
    setKind('api_key')
    setProvider('')
    resetFields()
    resetCreateMutation()
  }, [open, resetCreateMutation])

  useEffect(() => () => createAbortRef.current?.abort(), [])

  useEffect(() => {
    if (kind !== 'oauth') return
    setProvider((current) => providers.some((item) => item.provider === current)
      ? current
      : providers[0]?.provider ?? '')
  }, [kind, providers])

  function resetFields() {
    setSecret('')
    setPriority('0')
    setWeight('10')
    setShowSecret(false)
    setInvalidSecret(false)
    setInvalidSchedule(false)
    setCreateFailed(false)
    setCreateBusy(false)
  }

  const selectKind = (nextKind: CredentialFormKind) => {
    if (createBusy) return
    setKind(nextKind)
    setSecret('')
    setShowSecret(false)
    setInvalidSecret(false)
    setCreateFailed(false)
    createMutation.reset()
  }

  const addCredential = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const schedule = { priority: Number(priority), weight: Number(weight) }
    const secretValid = isValidCredentialSecret(secret)
    const scheduleValid = priority.trim() !== ''
      && weight.trim() !== ''
      && isValidCredentialSchedule(schedule)
    const providerValid = kind === 'api_key' || provider !== ''
    setInvalidSecret(!secretValid)
    setInvalidSchedule(!scheduleValid)
    if (!channel || !secretValid || !scheduleValid || !providerValid) return
    createAbortRef.current?.abort()
    const controller = new AbortController()
    createAbortRef.current = controller
    setCreateFailed(false)
    setCreateBusy(true)
    try {
      await createMutation.mutateAsync({
        channelId: channel.id,
        body: kind === 'api_key'
          ? createApiKeyRequest(secret, schedule)
          : createOAuthCredentialRequest(secret, provider, schedule),
        signal: controller.signal,
      })
      createMutation.reset()
      setSecret('')
      setPriority('0')
      setWeight('10')
      setShowSecret(false)
    } catch {
      createMutation.reset()
      if (controller.signal.aborted) return
      setCreateFailed(true)
      // Mutation 只呈现固定错误，明文保留在输入框中供用户确认后重试。
    } finally {
      if (createAbortRef.current === controller) {
        createAbortRef.current = undefined
        setCreateBusy(false)
      }
    }
  }

  const oauthUnavailable = kind === 'oauth'
    && (provider === '' || providersQuery.isPending || providersQuery.isError || providers.length === 0)

  return (
    <form className="grid gap-3 border-b border-[var(--hairline)] p-4" onSubmit={addCredential} noValidate>
      <div>
        <h3 className="text-xs font-semibold">{t('channels.credentials.addTitle')}</h3>
        <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.credentials.addHint')}</p>
      </div>

      <div className="grid grid-cols-2 gap-1 rounded-lg bg-surface-2 p-1" role="radiogroup" aria-label={t('channels.credentials.kindLabel')}>
        <CredentialKindButton active={kind === 'api_key'} disabled={createBusy} icon={KeyRound} label={t('channels.credentials.kind.api_key')} onClick={() => selectKind('api_key')} />
        <CredentialKindButton active={kind === 'oauth'} disabled={createBusy} icon={Link2} label={t('channels.credentials.kind.oauth')} onClick={() => selectKind('oauth')} />
      </div>

      {kind === 'oauth' ? (
        <div className="grid gap-1.5">
          <Label htmlFor="channel-credential-provider">{t('channels.credentials.oauth.provider')}</Label>
          {providersQuery.isPending ? (
            <Skeleton className="h-8 rounded-lg" aria-label={t('channels.credentials.oauth.providersLoading')} />
          ) : providersQuery.isError ? (
            <div role="alert" className="flex items-center justify-between gap-3 rounded-lg border border-destructive/25 bg-destructive/8 px-3 py-2">
              <p className="text-xs text-destructive">{t('channels.credentials.oauth.providersFailed')}</p>
              <Button type="button" size="icon-xs" variant="ghost" disabled={createBusy} aria-label={t('channels.actions.retry')} onClick={() => providersQuery.refetch()}><RefreshCw aria-hidden="true" /></Button>
            </div>
          ) : providers.length === 0 ? (
            <div className="rounded-lg border border-[var(--hairline)] bg-surface-2 px-3 py-2 text-xs text-muted-foreground">
              {t('channels.credentials.oauth.providersEmpty')}
            </div>
          ) : (
            <>
              <Select id="channel-credential-provider" value={provider} disabled={createBusy} onChange={(event) => { setProvider(event.target.value as AdminOAuthProvider); createMutation.reset() }}>
                {providers.map((item) => <option key={item.provider} value={item.provider}>{t(`channels.credentials.oauth.providers.${item.provider}`)}</option>)}
              </Select>
              {selectedProvider ? (
                <div className="flex items-center gap-2">
                  <Badge className={cn('border-transparent', selectedProvider.loopback_listener_ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning')}>
                    {t(selectedProvider.loopback_listener_ready ? 'channels.credentials.oauth.automaticCallback' : 'channels.credentials.oauth.manualCallback')}
                  </Badge>
                  <span className="text-[0.6875rem] text-muted-foreground">{t('channels.credentials.oauth.callbackPort', { port: selectedProvider.callback_port })}</span>
                </div>
              ) : null}
            </>
          )}
        </div>
      ) : null}

      <div className="grid gap-1.5">
        <Label htmlFor="channel-credential-secret">{t(kind === 'api_key' ? 'channels.credentials.apiKey' : 'channels.credentials.oauth.accessToken')}</Label>
        <div className="relative">
          <Input
            id="channel-credential-secret"
            type={showSecret ? 'text' : 'password'}
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            maxLength={16 * 1024}
            disabled={createBusy}
            className="pr-10 font-mono"
            value={secret}
            aria-invalid={invalidSecret}
            onChange={(event) => {
              setSecret(event.target.value)
              setInvalidSecret(false)
              setCreateFailed(false)
              createMutation.reset()
            }}
          />
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground"
            aria-label={t(showSecret ? 'channels.actions.hideApiKey' : 'channels.actions.showApiKey')}
            title={t(showSecret ? 'channels.actions.hideApiKey' : 'channels.actions.showApiKey')}
            aria-pressed={showSecret}
            disabled={createBusy}
            onClick={() => setShowSecret((current) => !current)}
          >
            {showSecret ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
          </Button>
        </div>
        {invalidSecret ? <p className="text-xs text-destructive">{t(kind === 'api_key' ? 'channels.validation.apiKey' : 'channels.validation.oauthAccessToken')}</p> : null}
        {kind === 'oauth' ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.credentials.oauth.accessTokenHint')}</p> : null}
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor="channel-credential-priority">{t('channels.fields.priority')}</Label>
          <Input id="channel-credential-priority" type="number" step={1} value={priority} disabled={createBusy} aria-invalid={invalidSchedule} onChange={(event) => { setPriority(event.target.value); setInvalidSchedule(false) }} />
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="channel-credential-weight">{t('channels.fields.weight')}</Label>
          <Input id="channel-credential-weight" type="number" min={0} step={1} value={weight} disabled={createBusy} aria-invalid={invalidSchedule} onChange={(event) => { setWeight(event.target.value); setInvalidSchedule(false) }} />
        </div>
      </div>
      {invalidSchedule ? <p className="text-xs text-destructive">{t('channels.validation.schedule')}</p> : null}
      {createFailed ? <p role="alert" className="text-xs text-destructive">{t(kind === 'api_key' ? 'channels.credentials.addFailed' : 'channels.credentials.oauth.addFailed')}</p> : null}
      <Button type="submit" className="w-fit" disabled={createBusy || oauthUnavailable}>
        {createBusy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}
        {t(kind === 'api_key' ? 'channels.credentials.addApiKey' : 'channels.credentials.oauth.add')}
      </Button>
    </form>
  )
}

function CredentialKindButton({ active, disabled, icon: Icon, label, onClick }: {
  active: boolean
  disabled: boolean
  icon: typeof KeyRound
  label: string
  onClick: () => void
}) {
  return (
    <Button
      type="button"
      size="sm"
      variant="ghost"
      role="radio"
      aria-checked={active}
      disabled={disabled}
      className={cn('w-full', active && 'bg-background text-foreground shadow-xs hover:bg-background')}
      onClick={onClick}
    >
      <Icon aria-hidden="true" />
      {label}
    </Button>
  )
}
