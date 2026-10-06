import type { ReactNode } from 'react'

import { Label } from '@/components/ui/label'

type UserFormFieldProps = {
  id: string
  label: string
  error?: string
  hint?: string
  children: ReactNode
}

/** 统一用户表单的标签、帮助文案和行内错误布局。 */
export function UserFormField({ id, label, error, hint, children }: UserFormFieldProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
