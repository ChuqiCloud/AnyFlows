import assert from 'node:assert/strict'
import test from 'node:test'
import { merchantWithdrawalAmount, parsePoolRatio, positiveInteger } from '../src/features/merchant-pool/merchant-pool-model.ts'

test('商户提现精确转换展示金额，不允许浮点舍入产生额外资金', () => {
  assert.equal(merchantWithdrawalAmount('0.01', '500000'), 5000)
  assert.equal(merchantWithdrawalAmount('1.2345', '10000'), 12345)
  assert.equal(merchantWithdrawalAmount('0.000002', '500000'), 1)
  assert.equal(merchantWithdrawalAmount('0.000001', '500000'), undefined)
  assert.equal(merchantWithdrawalAmount('0', '500000'), undefined)
  assert.equal(merchantWithdrawalAmount('-1', '500000'), undefined)
  assert.equal(merchantWithdrawalAmount('1e3', '500000'), undefined)
  assert.equal(merchantWithdrawalAmount('9007199254740992', '1'), undefined)
  assert.equal(merchantWithdrawalAmount('1', '0'), undefined)
})

test('商户销售倍率和标识拒绝精度溢出与指数输入', () => {
  assert.equal(parsePoolRatio('0.123456'), 123456)
  assert.equal(parsePoolRatio('0'), 0)
  assert.equal(parsePoolRatio('0.1234567'), undefined)
  assert.equal(parsePoolRatio('1e3'), undefined)
  assert.equal(parsePoolRatio('9007199254.740992'), undefined)
  assert.equal(positiveInteger('12'), 12)
  assert.equal(positiveInteger('1e2'), undefined)
  assert.equal(positiveInteger('01'), undefined)
  assert.equal(positiveInteger('9007199254740992'), undefined)
})
