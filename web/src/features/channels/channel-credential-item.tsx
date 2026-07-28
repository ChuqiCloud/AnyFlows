import type { ComponentProps } from 'react'
import { useState } from 'react'
import { KeyRound, LoaderCircle, Pencil, Power, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import {
  credentialRuntimeState,
  isValidCredentialSchedule,
  updateCredentialScheduleRequest,
  updateCredentialStatusRequest,
  type CredentialSchedule,
} from './channel-credential-model'

type ChannelCredentialItemProps = {
  credential: AdminCredential
  language: string
  pending: boolean
  onToggle: (credential: AdminCredential) => void
  onSchedule: (credential: AdminCredential, schedule: CredentialSchedule) => Promise<boolean>
  onDelete: (credential: AdminCredential) => void
}

const stateStyles = {
  available: 'bg-success/10 text-success',
  autoDisabled: 'bg-destructive/10 text-destructive',
  cooling: 'bg-warning/10 text-warning',
  disabled: 'bg-surface-2 text-muted-foreground',
  paused: 'bg-surface-2 text-muted-foreground',
  pending: 'bg-info/10 text-info',
} as const

/** 展示真实凭据状态，并提供不接触明文的调度参数编辑入口。 */
export function ChannelCredentialItem({ credential, language, pending, onToggle, onSchedule, onDelete }: ChannelCredentialItemProps) {
  const { t } = useTranslation()
  const [editing, setEditing] = useState(false)
  const [priority, setPriority] = useState(String(credential.priority))
  const [weight, setWeight] = useState(String(credential.weight))
  const [invalid, setInvalid] = useState(false)
  const runtimeState = credentialRuntimeState(credential)
  const scheduleEditable = updateCredentialScheduleRequest(credential, {
    priority: credential.priority,
    weight: credential.weight,
  }) !== undefined

  const saveSchedule = async () => {
    const schedule = { priority: Number(priority), weight: Number(weight) }
    const valid = priority.trim() !== '' && weight.trim() !== '' && isValidCredentialSchedule(schedule)
    setInvalid(!valid)
    if (!valid) return
    if (await onSchedule(credential, schedule)) setEditing(false)
  }

  return (
    <article className="py-3">
      <div className="flex items-start gap-3">
        <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><KeyRound className="size-4" aria-hidden="true" /></div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <h4 className="text-xs font-medium">{t('channels.credentials.itemName', { id: credential.id })}</h4>
            <Badge className={cn('border-transparent', stateStyles[runtimeState])}>{t(`channels.credentials.state.${runtimeState}`)}</Badge>
            <Badge>{t(`channels.credentials.kind.${credential.kind}`)}</Badge>
          </div>
          <div className="mt-1.5 flex flex-wrap gap-1 tabular-nums">
            <Badge>{t('channels.values.priority', { value: credential.priority })}</Badge>
            <Badge>{t('channels.values.weight', { value: credential.weight })}</Badge>
          </div>
          <p className="mt-1.5 truncate text-[0.6875rem] text-muted-foreground">
            {credential.last_used_at === null
              ? t('channels.credentials.neverUsed')
              : t('channels.credentials.lastUsed', { value: formatTimestamp(credential.last_used_at, language) })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <IconAction label={t(credential.status === 'enabled' ? 'channels.credentials.disable' : 'channels.credentials.enable')} disabled={pending || updateCredentialStatusRequest(credential, 'enabled') === undefined} onClick={() => onToggle(credential)}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Power aria-hidden="true" />}</IconAction>
          <IconAction label={t('channels.credentials.editSchedule')} disabled={pending || !scheduleEditable} onClick={() => { setPriority(String(credential.priority)); setWeight(String(credential.weight)); setInvalid(false); setEditing((current) => !current) }}><Pencil aria-hidden="true" /></IconAction>
          <IconAction label={t('channels.credentials.delete')} className="hover:text-destructive" onClick={() => onDelete(credential)}><Trash2 aria-hidden="true" /></IconAction>
        </div>
      </div>
      {editing ? (
        <div className="mt-3 grid gap-3 rounded-lg bg-surface-2 p-3 sm:grid-cols-2">
          <div className="grid gap-1.5"><Label htmlFor={`credential-${credential.id}-priority`}>{t('channels.fields.priority')}</Label><Input id={`credential-${credential.id}-priority`} type="number" step={1} value={priority} aria-invalid={invalid} onChange={(event) => { setPriority(event.target.value); setInvalid(false) }} /></div>
          <div className="grid gap-1.5"><Label htmlFor={`credential-${credential.id}-weight`}>{t('channels.fields.weight')}</Label><Input id={`credential-${credential.id}-weight`} type="number" min={0} step={1} value={weight} aria-invalid={invalid} onChange={(event) => { setWeight(event.target.value); setInvalid(false) }} /></div>
          {invalid ? <p className="text-xs text-destructive sm:col-span-2">{t('channels.validation.schedule')}</p> : null}
          <div className="flex justify-end gap-2 sm:col-span-2"><Button type="button" size="sm" variant="secondary" onClick={() => setEditing(false)}>{t('channels.actions.cancel')}</Button><Button type="button" size="sm" disabled={pending} onClick={saveSchedule}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('channels.actions.save')}</Button></div>
        </div>
      ) : null}
    </article>
  )
}

function IconAction({ label, className, children, ...props }: ComponentProps<typeof Button> & { label: string }) {
  return <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className={className} aria-label={label} {...props}>{children}</Button></TooltipTrigger><TooltipContent>{label}</TooltipContent></Tooltip>
}

function formatTimestamp(timestamp: number, language: string) {
  return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(timestamp * 1000)
}
