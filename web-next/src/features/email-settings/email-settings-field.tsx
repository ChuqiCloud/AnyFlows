import type { ReactNode } from 'react'

type SettingsSectionProps = {
  title: string
  description: string
  className?: string
  children: ReactNode
}

/** 统一系统设置分组的标题、说明和内容间距。 */
export function SettingsSection({ title, description, className, children }: SettingsSectionProps) {
  return (
    <section className={className}>
      <div className="px-4 pb-3 pt-4">
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      <div className="grid gap-4 px-4 pb-5">{children}</div>
    </section>
  )
}

type SettingsFieldProps = {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}

/** 保持标签、控件、帮助和错误信息的可访问顺序。 */
export function SettingsField({ id, label, hint, error, children }: SettingsFieldProps) {
  return (
    <div className="grid content-start gap-1.5">
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
