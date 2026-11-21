import { Button } from '@heroui/react'
import { CodeXml } from 'lucide-react'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  emailTemplateError,
  emailTemplateKinds,
  emailTemplateValues,
  renderEmailTemplatePreview,
  starterTemplate,
  type EmailTemplateKind,
} from './email-template-model'

type EmailTemplateEditorProps = {
  templates: Record<string, string>
  onChange: (templates: Record<string, string>) => void
  disabled: boolean
}

export function EmailTemplateEditor({ templates, onChange, disabled }: EmailTemplateEditorProps) {
  const { t } = useTranslation()
  const [kind, setKind] = useState<EmailTemplateKind>('registration')
  const [showPreview, setShowPreview] = useState(false)
  const editor = useRef<HTMLTextAreaElement>(null)
  const source = templates[kind] ?? ''
  const values = emailTemplateValues(kind)
  const error = emailTemplateError(kind, source)
  const update = (next: string) => {
    const updated = { ...templates }
    if (next) updated[kind] = next
    else delete updated[kind]
    onChange(updated)
  }
  const insert = (key: string) => {
    const element = editor.current
    if (!element) return
    const start = element.selectionStart
    const end = element.selectionEnd
    const token = `{{${key}}}`
    update(source.slice(0, start) + token + source.slice(end))
    requestAnimationFrame(() => {
      element.focus()
      element.setSelectionRange(start + token.length, start + token.length)
    })
  }
  const kindLabel = t(`emailSettings.templates.kinds.${kind}`)
  const preview = showPreview && source.trim() && !error
    ? renderEmailTemplatePreview(source, {
        ...values,
        subject: kindLabel,
        body: t(`emailSettings.templates.samples.${kind}`, values),
      })
    : undefined

  return (
    <section aria-labelledby="email-template-title" className="grid gap-3 border-t border-[var(--hairline)] p-4">
      <div className="flex items-center gap-2">
        <CodeXml className="size-4 text-info" aria-hidden="true" />
        <h3 id="email-template-title" className="text-sm font-semibold">{t('emailSettings.templates.title')}</h3>
      </div>
      <div className="grid gap-1.5">
        <label className="text-xs font-medium" htmlFor="email-template-kind">{t('emailSettings.templates.kind')}</label>
        <select id="email-template-kind" value={kind} disabled={disabled} onChange={(event) => setKind(event.target.value as EmailTemplateKind)} className="h-9 w-full max-w-72 rounded-md border border-[var(--hairline)] bg-surface-1 px-2 text-sm">
          {emailTemplateKinds.map((item) => <option key={item} value={item}>{t(`emailSettings.templates.kinds.${item}`)}</option>)}
        </select>
      </div>
      <div className="grid gap-1.5">
        <label className="text-xs font-medium" htmlFor="email-template-html">{t('emailSettings.templates.html')}</label>
        <textarea
          key={kind}
          ref={editor}
          id="email-template-html"
          aria-invalid={Boolean(error)}
          aria-describedby="email-template-hint"
          rows={8}
          spellCheck={false}
          value={source}
          disabled={disabled}
          onChange={(event) => update(event.target.value)}
          className="w-full resize-y rounded-lg border border-[var(--hairline)] bg-surface-2/35 p-2.5 font-mono text-xs leading-relaxed focus-visible:outline-2 focus-visible:outline-primary"
        />
        <p id="email-template-hint" className="text-xs text-muted-foreground">{t('emailSettings.templates.hint')}</p>
        {error ? <p role="alert" className="text-xs text-destructive">{t(`emailSettings.templates.errors.${error}`)}</p> : null}
      </div>
      <div role="group" aria-label={t('emailSettings.templates.variables')} className="flex flex-wrap gap-1.5">
        {Object.keys(values).map((key) => (
          <Button key={key} type="button" size="sm" variant="flat" className="h-7 min-w-0 px-2 font-mono text-xs" isDisabled={disabled} onPress={() => insert(key)}>
            {`{{${key}}}`}
          </Button>
        ))}
      </div>
      <div className="flex flex-wrap gap-2">
        <Button type="button" size="sm" variant="bordered" isDisabled={disabled || Boolean(source)} onPress={() => update(starterTemplate)}>{t('emailSettings.templates.useExample')}</Button>
        <Button type="button" size="sm" variant="light" isDisabled={disabled || !source} onPress={() => update('')}>{t('emailSettings.templates.restore')}</Button>
        <Button type="button" size="sm" variant="light" aria-pressed={showPreview} isDisabled={!showPreview && (!source.trim() || Boolean(error))} onPress={() => setShowPreview(!showPreview)}>{t('emailSettings.templates.preview')}</Button>
      </div>
      {preview ? (
        <iframe
          title={t('emailSettings.templates.preview')}
          srcDoc={preview}
          sandbox=""
          tabIndex={-1}
          referrerPolicy="no-referrer"
          className="pointer-events-none h-64 w-full rounded-lg border border-[var(--hairline)] bg-white"
        />
      ) : null}
    </section>
  )
}
