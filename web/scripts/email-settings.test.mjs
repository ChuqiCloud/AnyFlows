import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildEmailSettingsSchema,
  emailSettingsValues,
  isValidEmail,
  toEmailSettingsRequest,
} from '../src/features/email-settings/email-settings-form-model.ts'

const messages = {
  host: 'host',
  port: 'port',
  username: 'username',
  password: 'password',
  passwordRequired: 'password required',
  passwordWithoutUser: 'password without user',
  fromAddress: 'from address',
  fromAddressRequired: 'from address required',
  fromName: 'from name',
  replyTo: 'reply to',
  timeout: 'timeout',
}

const persisted = {
  enabled: false,
  host: '',
  port: 587,
  tls_mode: 'start_tls',
  username: null,
  password_configured: false,
  from_address: '',
  from_name: null,
  reply_to: null,
  timeout_seconds: 10,
  version: 1,
  delivery_ready: false,
}

test('邮件设置路由指向管理员系统设置子域', () => {
  assert.deepEqual(routeFromHash('#/console/system-settings/email'), { view: 'email-settings' })
})

test('缺省关闭设置可编辑，但启用前必须补全投递字段', () => {
  const values = emailSettingsValues(persisted)
  assert.equal(buildEmailSettingsSchema(false, messages).safeParse(values).success, true)
  assert.equal(buildEmailSettingsSchema(false, messages).safeParse({
    ...values,
    enabled: true,
  }).success, false)
})

test('认证用户名与密码遵守保留、替换和清除语义', () => {
  const base = {
    ...emailSettingsValues(persisted),
    host: 'smtp.example.com',
    fromAddress: 'from@example.com',
    username: 'mailer@example.com',
  }
  assert.equal(buildEmailSettingsSchema(false, messages).safeParse(base).success, false)
  assert.equal(buildEmailSettingsSchema(true, messages).safeParse(base).success, true)
  assert.equal(buildEmailSettingsSchema(false, messages).safeParse({
    ...base,
    username: '',
    password: 'new secret',
  }).success, false)

  assert.deepEqual(toEmailSettingsRequest({
    ...base,
    username: '',
    password: '',
  }), {
    enabled: false,
    host: 'smtp.example.com',
    port: 587,
    tls_mode: 'start_tls',
    username: null,
    password: null,
    from_address: 'from@example.com',
    from_name: null,
    reply_to: null,
    timeout_seconds: 10,
  })
})

test('测试收件人与 SMTP 主机使用闭合校验', () => {
  assert.equal(isValidEmail('recipient@example.com'), true)
  assert.equal(isValidEmail('recipient @example.com'), false)
  const values = {
    ...emailSettingsValues(persisted),
    host: 'https://smtp.example.com',
  }
  assert.equal(buildEmailSettingsSchema(false, messages).safeParse(values).success, false)
})
