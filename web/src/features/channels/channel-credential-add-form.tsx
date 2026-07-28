import type { FormEvent } from 'react'
import { useEffect, useState } from 'react'
import { Eye, EyeOff, LoaderCircle, Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { useCreateAdminCredential } from './channel-api'
import {
  createApiKeyRequest,
  isValidApiKey,
  isValidCredentialSchedule,
} from './channel-credential-model'

type ChannelCredentialAddFormProps = {
  channel?: AdminChannel
  open: boolean
}

/** 添加 API Key 时同时收集可直接生效的优先级与权重。 */
export function ChannelCredentialAddForm({ channel, open }: ChannelCredentialAddFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminCredential()
  const [secret, setSecret] = useState('')
  const [priority, setPriority] = useState('0')
  const [weight, setWeight] = useState('10')
  const [showSecret, setShowSecret] = useState(false)
  const [invalidSecret, setInvalidSecret] = useState(false)
  const [invalidSchedule, setInvalidSchedule] = useState(false)

  useEffect(() => {
    setSecret('')
    setPriority('0')
    setWeight('10')
    setShowSecret(false)
    setInvalidSecret(false)
    setInvalidSchedule(false)
  }, [channel?.id])

  useEffect(() => {
    if (open) return
    setSecret('')
    setPriority('0')
    setWeight('10')
    setShowSecret(false)
    setInvalidSecret(false)
    setInvalidSchedule(false)
  }, [open])

  const addCredential = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const schedule = { priority: Number(priority), weight: Number(weight) }
    const secretValid = isValidApiKey(secret)
    const scheduleValid = priority.trim() !== ''
      && weight.trim() !== ''
      && isValidCredentialSchedule(schedule)
    setInvalidSecret(!secretValid)
    setInvalidSchedule(!scheduleValid)
    if (!channel || !secretValid || !scheduleValid) return
    try {
      await createMutation.mutateAsync({
        channelId: channel.id,
        body: createApiKeyRequest(secret, schedule),
      })
      setSecret('')
      setPriority('0')
      setWeight('10')
      setShowSecret(false)
    } catch {
      // Mutation 只呈现固定错误，明文保留在输入框中供用户确认后重试。
    }
  }

  return (
    <form className="grid gap-3 border-b border-[var(--hairline)] p-4" onSubmit={addCredential} noValidate>
      <div>
        <h3 className="text-xs font-semibold">{t('channels.credentials.addTitle')}</h3>
        <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.credentials.addHint')}</p>
      </div>
      <div className="grid gap-1.5">
        <Label htmlFor="channel-credential-secret">{t('channels.credentials.apiKey')}</Label>
        <div className="relative">
          <Input
            id="channel-credential-secret"
            type={showSecret ? 'text' : 'password'}
            autoComplete="off"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            className="pr-10 font-mono"
            value={secret}
            aria-invalid={invalidSecret}
            onChange={(event) => {
              setSecret(event.target.value)
              setInvalidSecret(false)
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
            onClick={() => setShowSecret((current) => !current)}
          >
            {showSecret ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
          </Button>
        </div>
        {invalidSecret ? <p className="text-xs text-destructive">{t('channels.validation.apiKey')}</p> : null}
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="grid gap-1.5">
          <Label htmlFor="channel-credential-priority">{t('channels.fields.priority')}</Label>
          <Input id="channel-credential-priority" type="number" step={1} value={priority} aria-invalid={invalidSchedule} onChange={(event) => { setPriority(event.target.value); setInvalidSchedule(false) }} />
        </div>
        <div className="grid gap-1.5">
          <Label htmlFor="channel-credential-weight">{t('channels.fields.weight')}</Label>
          <Input id="channel-credential-weight" type="number" min={0} step={1} value={weight} aria-invalid={invalidSchedule} onChange={(event) => { setWeight(event.target.value); setInvalidSchedule(false) }} />
        </div>
      </div>
      {invalidSchedule ? <p className="text-xs text-destructive">{t('channels.validation.schedule')}</p> : null}
      {createMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('channels.credentials.addFailed')}</p> : null}
      <Button type="submit" className="w-fit" disabled={createMutation.isPending}>
        {createMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}
        {t('channels.credentials.add')}
      </Button>
    </form>
  )
}
