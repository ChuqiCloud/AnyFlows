export const emailTemplateKinds = [
  'registration', 'email_binding', 'password_reset',
  'balance_alert', 'subscription_balance_alert', 'smtp_test',
] as const

export type EmailTemplateKind = (typeof emailTemplateKinds)[number]

const commonValues = { recipient: 'member@example.com', subject: '', body: '' }
const kindValues = {
  registration: { code: '123456', expires_minutes: '10' },
  email_binding: { code: '123456', expires_minutes: '10' },
  password_reset: { site_name: 'AnyFlows', reset_url: 'https://example.com/reset?token=example', expires_minutes: '30' },
  balance_alert: { site_name: 'AnyFlows', username: 'demo-user', balance: '12000', threshold: '20000' },
  subscription_balance_alert: {
    site_name: 'AnyFlows', username: 'demo-user', plan_name: 'Pro', remaining: '5000',
    quota: '50000', threshold_percent: '10', window_end: '2026-12-01 00:00 UTC',
  },
  smtp_test: {},
} satisfies Record<EmailTemplateKind, Record<string, string>>

const required = {
  registration: 'code',
  email_binding: 'code',
  password_reset: 'reset_url',
} satisfies Partial<Record<EmailTemplateKind, string>>

export const starterTemplate = '<h2>{{subject}}</h2>\n<div style="white-space: pre-wrap">{{body}}</div>'

export function emailTemplateValues(kind: EmailTemplateKind): Record<string, string> {
  return { ...commonValues, ...kindValues[kind] }
}

export function emailTemplateError(kind: EmailTemplateKind, html: string) {
  if (new TextEncoder().encode(html).length > 16_384 || /[^\P{Cc}\t\n\r]/u.test(html)) return 'size'
  if (!html.trim()) return
  const variables = emailTemplateValues(kind)
  const requiredKey = kind in required ? required[kind as keyof typeof required] : undefined
  let hasContent = !requiredKey
  let source = html
  let occurrences = 0
  for (;;) {
    const start = source.indexOf('{{')
    if (start < 0) return hasContent ? undefined : 'required'
    const end = source.indexOf('}}', start + 2)
    if (end < 0 || ++occurrences > 64) return 'variable'
    const key = source.slice(start + 2, end).trim()
    if (!Object.hasOwn(variables, key)) return 'variable'
    if (key === 'body' || key === requiredKey) hasContent = true
    source = source.slice(end + 2)
  }
}

const escapeValues: Record<string, string> = {
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  '`': '&#96;', '=': '&#61;', ' ': '&#32;', '\t': '&#9;', '\n': '&#10;', '\r': '&#13;',
}

export function renderEmailTemplatePreview(template: string, values: Record<string, string>) {
  const html = template.replace(/\{\{([^]*?)\}\}/g, (_, name: string) => (
    (values[name.trim()] ?? '').replace(/[&<>"'`= \t\r\n]/g, (character) => escapeValues[character])
  ))
  // The preview is sandboxed and used only with sample data. Remote assets are disabled.
  return '<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src \'none\'; style-src \'unsafe-inline\'; img-src data:; base-uri \'none\'; form-action \'none\'">' + html
}
