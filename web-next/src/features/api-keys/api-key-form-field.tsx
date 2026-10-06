import type { ReactNode } from 'react'
import { Switch } from '@heroui/react'

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
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
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
        <label className="text-xs font-medium leading-none text-foreground" htmlFor={props.id}>{props.label}</label>
        <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{props.hint}</p>
      </div>
      <Switch
        aria-label={props.label}
        id={props.id}
        isDisabled={props.disabled}
        isSelected={props.checked}
        size="sm"
        onValueChange={props.onCheckedChange}
      />
    </div>
  )
}
