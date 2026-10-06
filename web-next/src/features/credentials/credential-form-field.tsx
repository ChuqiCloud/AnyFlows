import type { ComponentProps, ReactNode } from 'react'
import { useState } from 'react'
import { Button, Input } from '@heroui/react'
import { Eye, EyeOff } from 'lucide-react'
import { useTranslation } from 'react-i18next'

type CredentialFieldProps = {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}

export function CredentialField({ id, label, hint, error, children }: CredentialFieldProps) {
  return (
    <div className="grid gap-1.5">
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p role="alert" className="text-xs text-destructive">{error}</p> : hint ? (
        <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p>
      ) : null}
    </div>
  )
}

/** 敏感单行输入默认遮蔽，明文只停留在当前表单内存。 */
export function CredentialSecretInput(props: ComponentProps<typeof Input>) {
  const { t } = useTranslation()
  const [visible, setVisible] = useState(false)
  return (
    <div className="relative">
      <Input
        {...props}
        autoCapitalize="none"
        autoComplete="off"
        autoCorrect="off"
        classNames={{ input: `pr-10 font-mono ${props.classNames?.input ?? ''}` }}
        size="sm"
        spellCheck="false"
        type={visible ? 'text' : 'password'}
      />
      <Button
        isIconOnly
        aria-label={t(visible ? 'credentials.actions.hideSecret' : 'credentials.actions.showSecret')}
        aria-pressed={visible}
        className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground"
        isDisabled={props.disabled}
        size="sm"
        type="button"
        variant="light"
        onClick={() => setVisible((current) => !current)}
      >
        {visible ? <EyeOff className="size-3.5" aria-hidden="true" /> : <Eye className="size-3.5" aria-hidden="true" />}
      </Button>
    </div>
  )
}

export function CredentialSection({ title, description, children }: {
  title: string
  description?: string
  children: ReactNode
}) {
  return (
    <section className="grid gap-3 border-t border-[var(--hairline)] px-5 py-4 first:border-t-0">
      <div>
        <h3 className="text-xs font-semibold">{title}</h3>
        {description ? <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{description}</p> : null}
      </div>
      {children}
    </section>
  )
}
