import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildUserFormSchema,
  defaultUserValues,
  toUserCreateRequest,
  toUserUpdateRequest,
  userRequestWithStatus,
} from '../src/features/users/user-form-model.ts'
import {
  buildWalletAdjustmentSchema,
  createWalletAdjustmentAttempt,
  createWalletEventId,
  defaultWalletAdjustmentValues,
  previewWalletBalance,
  toWalletAdjustmentRequest,
  walletAdjustmentAttemptForValues,
} from '../src/features/users/wallet-form-model.ts'

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
  assert.equal(buildUserFormSchema('update', messages).safeParse({
    ...values,
    quota: Number.MAX_SAFE_INTEGER + 1,
  }).success, true)
})

test('创建用户把空邮箱和可选限制收敛为 null', () => {
  const request = toUserCreateRequest({
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

test('编辑与列表启停都不再提交钱包余额', () => {
  const values = {
    ...defaultUserValues(),
    username: 'operator',
    password: '',
    defaultGroupId: '3',
    quota: 900,
    rpmLimit: '60',
    concurrency: '4',
  }
  const update = toUserUpdateRequest(values)
  assert.equal(Object.hasOwn(update, 'quota'), false)

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
  assert.equal(Object.hasOwn(request, 'quota'), false)
  assert.equal(request.rpm_limit, 60)
  assert.equal(request.concurrency, 4)
})

test('钱包事件键为非零 128 位小写十六进制且避开 opening 命名空间', () => {
  const first = createWalletEventId()
  const second = createWalletEventId()
  assert.match(first, /^(?!0{32}$)[0-9a-f]{32}$/)
  assert.doesNotMatch(first, /^0000000000000001/)
  assert.notEqual(first, second)
})

test('调账字段不变时复用事件键，业务事实改变后才换键', () => {
  const values = { ...defaultWalletAdjustmentValues(), amount: '25', reason: '人工补偿' }
  const attempt = createWalletAdjustmentAttempt(values)
  assert.equal(walletAdjustmentAttemptForValues(attempt, { ...values }).eventId, attempt.eventId)

  const changed = walletAdjustmentAttemptForValues(attempt, { ...values, amount: '26' })
  assert.notEqual(changed.eventId, attempt.eventId)
})

test('结构化调账生成正确符号并在本地预判不足与精确溢出', () => {
  const eventId = createWalletEventId()
  const base = { ...defaultWalletAdjustmentValues(), amount: '25', reason: '人工调账' }
  assert.equal(toWalletAdjustmentRequest(base, eventId).quota_delta, 25)
  assert.equal(toWalletAdjustmentRequest({ ...base, direction: 'decrease' }, eventId).quota_delta, -25)
  assert.deepEqual(previewWalletBalance(100, 'decrease', '101'), { status: 'insufficient' })
  assert.deepEqual(previewWalletBalance(Number.MAX_SAFE_INTEGER, 'increase', '1'), { status: 'overflow' })
})

test('调账原因按 UTF-8 字节边界校验并拒绝零额度', () => {
  const schema = buildWalletAdjustmentSchema({ invalidAmount: 'amount', invalidReason: 'reason' })
  const valid = { direction: 'increase', amount: '1', reason: '人工调账' }
  assert.equal(schema.safeParse(valid).success, true)
  assert.equal(schema.safeParse({ ...valid, amount: '0' }).success, false)
  assert.equal(schema.safeParse({ ...valid, reason: ` ${valid.reason}` }).success, false)
  assert.equal(schema.safeParse({ ...valid, reason: '中'.repeat(167) }).success, false)
})
