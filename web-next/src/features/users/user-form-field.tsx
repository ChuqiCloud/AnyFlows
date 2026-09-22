import type { ReactNode } from 'react'

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
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
