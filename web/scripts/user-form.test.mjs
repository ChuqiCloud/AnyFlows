import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildUserFormSchema,
  defaultUserValues,
  toUserRequest,
  userRequestWithStatus,
} from '../src/features/users/user-form-model.ts'

const messages = {
  invalidUsername: 'username',
  invalidEmail: 'email',
  invalidPassword: 'password',
  invalidGroup: 'group',
  invalidNumber: 'number',
}

test('创建用户要求初始密码，编辑用户允许留空保留密码', () => {
  const values = {
    ...defaultUserValues(),
    username: 'reader',
    defaultGroupId: '1',
  }
  assert.equal(buildUserFormSchema('create', messages).safeParse(values).success, false)
  assert.equal(buildUserFormSchema('update', messages).safeParse(values).success, true)
})

test('用户表单把空邮箱和可选限制收敛为 null', () => {
  const request = toUserRequest({
    ...defaultUserValues(),
    username: 'reader',
    password: 'secret',
    defaultGroupId: '7',
  })
  assert.equal(request.email, null)
  assert.equal(request.rpm_limit, null)
  assert.equal(request.concurrency, null)
  assert.equal(request.default_group_id, 7)
})

test('列表启停保留角色、分组、额度和限流字段', () => {
  const request = userRequestWithStatus({
    id: 8,
    username: 'operator',
    email: 'operator@example.com',
    role: 'admin',
    status: 'enabled',
    default_group_id: 3,
    quota: 900,
    used_quota: 100,
    frozen_quota: 20,
    request_count: 77,
    rpm_limit: 60,
    concurrency: 4,
  }, 'disabled')
  assert.equal(request.status, 'disabled')
  assert.equal(request.password, null)
  assert.equal(request.role, 'admin')
  assert.equal(request.default_group_id, 3)
  assert.equal(request.quota, 900)
  assert.equal(request.rpm_limit, 60)
  assert.equal(request.concurrency, 4)
})
