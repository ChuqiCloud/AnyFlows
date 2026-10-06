import type { ComponentProps, ReactNode } from 'react'
import { useState } from 'react'
import { Eye, EyeOff } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'

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
      <Label htmlFor={id}>{label}</Label>
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
        type={visible ? 'text' : 'password'}
        autoComplete="off"
        autoCapitalize="none"
        autoCorrect="off"
        spellCheck={false}
        className={`pr-10 font-mono ${props.className ?? ''}`}
      />
      <Button
        type="button"
        variant="ghost"
        size="icon-sm"
        className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground"
        aria-label={t(visible ? 'credentials.actions.hideSecret' : 'credentials.actions.showSecret')}
        aria-pressed={visible}
        disabled={props.disabled}
        onClick={() => setVisible((current) => !current)}
      >
        {visible ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
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
