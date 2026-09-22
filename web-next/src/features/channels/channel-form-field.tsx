import type { ReactNode } from 'react'

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
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      {htmlFor === false
        ? <p className="text-xs font-medium">{label}</p>
        : <label className="text-xs font-medium leading-none text-foreground" htmlFor={htmlFor}>{label}</label>}
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? (
        <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  )
}
