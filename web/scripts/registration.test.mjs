import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildRegistrationFormSchema,
  defaultRegistrationFormValues,
  toRegistrationRequest,
} from '../src/features/registration/registration-form-model.ts'

const registrationMessages = {
  invalidUsername: 'username',
  invalidEmail: 'email',
  emailRequired: 'email-required',
  invalidVerificationCode: 'verification-code',
  verificationCodeRequired: 'verification-code-required',
  invalidInviteCode: 'invite-code',
  invalidPassword: 'password',
  passwordMismatch: 'password-mismatch',
}

test('公开注册与认证设置路由保持独立权限入口', () => {
  assert.deepEqual(routeFromHash('#/register'), { view: 'register' })
  assert.deepEqual(routeFromHash('#/register?invite=af-AAAAAAAAAAAAAAAAAAAAAA'), {
    view: 'register',
    inviteCode: 'af-AAAAAAAAAAAAAAAAAAAAAA',
  })
  assert.deepEqual(routeFromHash('#/console/system-settings/authentication'), {
    view: 'authentication-settings',
  })
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
  assert.equal(
    buildRegistrationFormSchema(false, registrationMessages).safeParse({
      ...values,
      inviteCode: 'invalid-invite',
    }).success,
    false,
  )
})

test('可选空邮箱归一化为 null 且不发送确认密码', () => {
  const request = toRegistrationRequest({
    username: 'reader',
    email: '',
    verificationCode: '',
    inviteCode: '',
    password: 'correct horse battery staple',
    passwordConfirmation: 'correct horse battery staple',
  })
  assert.deepEqual(request, {
    username: 'reader',
    email: null,
    verification_code: null,
    invite_code: null,
    password: 'correct horse battery staple',
  })
})
