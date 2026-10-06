import type { ReactNode } from 'react'

import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'

type ApiKeyFieldProps = {
  id: string
  label: string
  error?: string
  hint?: string
  children: ReactNode
}

export function ApiKeyField({ id, label, error, hint, children }: ApiKeyFieldProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

type ApiKeySwitchFieldProps = {
  id: string
  label: string
  hint: string
  checked: boolean
  disabled?: boolean
  onCheckedChange: (checked: boolean) => void
}

export function ApiKeySwitchField(props: ApiKeySwitchFieldProps) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
      <div>
        <Label htmlFor={props.id}>{props.label}</Label>
        <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{props.hint}</p>
      </div>
      <Switch
        id={props.id}
        checked={props.checked}
        disabled={props.disabled}
        onCheckedChange={props.onCheckedChange}
      />
    </div>
  )
}
