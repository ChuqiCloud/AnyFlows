import assert from 'node:assert/strict'
import test from 'node:test'

import { emailTemplateError, emailTemplateKinds, renderEmailTemplatePreview, starterTemplate } from '../src/features/email-settings/email-template-model.ts'
import { emailSettingsValues, toEmailSettingsRequest } from '../src/features/email-settings/email-settings-form-model.ts'

test('HTML email authoring checks allowed fields and action content', () => {
  for (const kind of emailTemplateKinds) assert.equal(emailTemplateError(kind, starterTemplate), undefined)
  assert.equal(emailTemplateError('password_reset', '<a href="{{reset_url}}">Reset</a>'), undefined)
  assert.equal(emailTemplateError('registration', '<p>{{expires_minutes}}</p>'), 'required')
  assert.equal(emailTemplateError('balance_alert', '{{code}}'), 'variable')
  assert.equal(emailTemplateError('smtp_test', '{{subject'), 'variable')
  assert.equal(emailTemplateError('smtp_test', '\x00'), 'size')
  assert.equal(emailTemplateError('smtp_test', '<p>\n\t{{body}}\n</p>'), undefined)
})

test('HTML variables are escaped in the preview and never evaluated again', () => {
  const preview = renderEmailTemplatePreview('<p>{{body}}</p>', { body: '<tag> {{code}}' })
  assert.match(preview, /default-src 'none'/)
  assert.match(preview, /<p>&lt;tag&gt;&#32;\{\{code\}\}<\/p>$/)
})

test('only edited HTML templates are included in an SMTP update', () => {
  const saved = { smtp_test: '<p>{{body}}</p>', registration: '<b>{{code}}</b>' }
  const values = emailSettingsValues({ html_templates: saved })
  assert.equal(toEmailSettingsRequest(values, saved).html_templates, undefined)
  assert.deepEqual(
    toEmailSettingsRequest({ ...values, htmlTemplates: { smtp_test: ' ', registration: saved.registration } }, saved).html_templates,
    { smtp_test: '' },
  )
})
