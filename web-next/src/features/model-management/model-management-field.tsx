import type { ReactNode } from 'react'

type ModelManagementFieldProps = {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}

/** 统一模型元数据表单的标签、提示和行内错误位置。 */
export function ModelManagementField({ id, label, hint, error, children }: ModelManagementFieldProps) {
  const descriptionId = error ? `${id}-error` : hint ? `${id}-hint` : undefined
  return (
    <div className="grid gap-2">
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p id={descriptionId} role="alert" className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p id={descriptionId} className="text-xs leading-5 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
