import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildRegistrationFormSchema,
  buildRegistrationSettingsSchema,
  defaultRegistrationFormValues,
  settingsValuesFromPolicy,
  toRegistrationPolicyRequest,
  toRegistrationRequest,
} from '../src/features/registration/registration-form-model.ts'

const registrationMessages = {
  invalidUsername: 'username',
  invalidEmail: 'email',
  emailRequired: 'email-required',
  invalidPassword: 'password',
  passwordMismatch: 'password-mismatch',
}

const settingsMessages = {
  invalidGroup: 'group',
  invalidQuota: 'quota',
  invalidAttempts: 'attempts',
  invalidWindow: 'window',
}

test('公开注册与管理员策略路由保持独立权限入口', () => {
  assert.deepEqual(routeFromHash('#/register'), { view: 'register' })
  assert.deepEqual(routeFromHash('#/console/registration'), { view: 'registration' })
})

test('邮箱必填策略与密码确认共同约束公开注册表单', () => {
  const values = {
    ...defaultRegistrationFormValues,
    username: 'reader',
    password: 'correct horse battery staple',
    passwordConfirmation: 'correct horse battery staple',
  }
  assert.equal(
    buildRegistrationFormSchema(false, registrationMessages).safeParse(values).success,
    true,
  )
  assert.equal(
    buildRegistrationFormSchema(true, registrationMessages).safeParse(values).success,
    false,
  )
  assert.equal(
    buildRegistrationFormSchema(true, registrationMessages).safeParse({
      ...values,
      email: 'reader@example.com',
      passwordConfirmation: 'different password',
    }).success,
    false,
  )
})

test('可选空邮箱归一化为 null 且不发送确认密码', () => {
  const request = toRegistrationRequest({
    username: 'reader',
    email: '',
    password: 'correct horse battery staple',
    passwordConfirmation: 'correct horse battery staple',
  })
  assert.deepEqual(request, {
    username: 'reader',
    email: null,
    password: 'correct horse battery staple',
  })
})

test('管理员策略表单只接受闭合窗口并精确转换整数', () => {
  const values = settingsValuesFromPolicy({
    enabled: true,
    default_group_id: 7,
    initial_quota: 500,
    email_required: true,
    rate_limit_attempts: 5,
    rate_limit_window_seconds: 3_600,
  })
  assert.equal(buildRegistrationSettingsSchema(settingsMessages).safeParse(values).success, true)
  assert.equal(buildRegistrationSettingsSchema(settingsMessages).safeParse({
    ...values,
    rateLimitWindowSeconds: '120',
  }).success, true)
  assert.equal(buildRegistrationSettingsSchema(settingsMessages).safeParse({
    ...values,
    rateLimitWindowSeconds: '59',
  }).success, false)
  assert.deepEqual(toRegistrationPolicyRequest(values), {
    enabled: true,
    default_group_id: 7,
    initial_quota: 500,
    email_required: true,
    rate_limit_attempts: 5,
    rate_limit_window_seconds: 3_600,
  })
})
