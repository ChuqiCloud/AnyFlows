import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  authenticationSettingsValues,
  buildAuthenticationSettingsSchema,
  toAuthenticationSettingsRequest,
} from '../src/features/authentication-settings/authentication-settings-form-model.ts'

const messages = {
  incompatibleCapabilities: 'capabilities',
  invalidGroup: 'group',
  invalidQuota: 'quota',
  invalidAttempts: 'attempts',
  invalidWindow: 'window',
}

test('认证设置使用系统设置路由并兼容旧注册设置书签', () => {
  assert.deepEqual(routeFromHash('#/console/system-settings/authentication'), {
    view: 'authentication-settings',
  })
  assert.deepEqual(routeFromHash('#/console/registration'), {
    view: 'authentication-settings',
  })
})

test('认证设置拒绝开启注册但关闭密码登录的非法组合', () => {
  const values = authenticationSettingsValues({
    password_login_enabled: true,
    registration_enabled: true,
    default_group_id: 7,
    initial_quota: 500,
    email_required: true,
    rate_limit_attempts: 5,
    rate_limit_window_seconds: 3_600,
    version: 2,
  })
  assert.equal(buildAuthenticationSettingsSchema(messages).safeParse(values).success, true)
  assert.equal(buildAuthenticationSettingsSchema(messages).safeParse({
    ...values,
    passwordLoginEnabled: false,
  }).success, false)
  assert.equal(buildAuthenticationSettingsSchema(messages).safeParse({
    ...values,
    rateLimitWindowSeconds: '59',
  }).success, false)
  assert.deepEqual(toAuthenticationSettingsRequest(values), {
    password_login_enabled: true,
    registration_enabled: true,
    default_group_id: 7,
    initial_quota: 500,
    email_required: true,
    rate_limit_attempts: 5,
    rate_limit_window_seconds: 3_600,
  })
})
