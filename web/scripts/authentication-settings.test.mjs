import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

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
  invalidRebateQuota: 'rebate-quota',
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
    invitation_rebate_quota: 25,
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
  assert.equal(buildAuthenticationSettingsSchema(messages).safeParse({
    ...values,
    invitationRebateQuota: -1,
  }).success, false)
  assert.deepEqual(toAuthenticationSettingsRequest(values), {
    password_login_enabled: true,
    registration_enabled: true,
    default_group_id: 7,
    initial_quota: 500,
    invitation_rebate_quota: 25,
    email_required: true,
    rate_limit_attempts: 5,
    rate_limit_window_seconds: 3_600,
  })
})

test('认证设置页提供不污染 hash 路由的可聚焦分段导航', () => {
  const page = readFileSync(new URL('../src/features/authentication-settings/authentication-settings-page.tsx', import.meta.url), 'utf8')
  const form = readFileSync(new URL('../src/features/authentication-settings/authentication-settings-form.tsx', import.meta.url), 'utf8')
  assert.match(page, /authenticationSettings\.navigation\.label/)
  assert.match(page, /focusSection\(section\.id\)/)
  assert.match(page, /adminOAuthLoginSettingsQueryPrefix/)
  assert.match(page, /invalidateQueries\(\{ queryKey: adminOAuthLoginSettingsQueryPrefix \}\)/)
  assert.match(page, /useIsFetching\(\{ queryKey: adminOAuthLoginSettingsQueryPrefix \}\)/)
  assert.match(page, /disabled=\{refreshing\}/)
  assert.match(page, /authentication-oauth/)
  assert.match(form, /id="authentication-core"/)
  assert.match(form, /id="authentication-allocation"/)
  assert.match(form, /id="authentication-protection"/)
  assert.doesNotMatch(page, /href="#authentication-/)
})
