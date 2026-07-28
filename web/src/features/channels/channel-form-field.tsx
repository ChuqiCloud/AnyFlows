import type { ReactNode } from 'react'

import { Label } from '@/components/ui/label'

type ChannelFormFieldProps = {
  id: string
  label: string
  error?: string
  hint?: string
  htmlFor?: string | false
  children: ReactNode
}

/** 统一渠道表单的标签、提示与行内错误布局。 */
export function ChannelFormField({ id, label, error, hint, htmlFor = id, children }: ChannelFormFieldProps) {
  return (
    <div className="grid gap-1.5">
      {htmlFor === false
        ? <p className="text-xs font-medium">{label}</p>
        : <Label htmlFor={htmlFor}>{label}</Label>}
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? (
        <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  )
}
