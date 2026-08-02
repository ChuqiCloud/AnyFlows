import assert from 'node:assert/strict'
import test from 'node:test'

import { shouldShowEmptyAssistantResponse } from '../src/features/playground/playground-status.ts'

test('仅成功完成且正文为空时展示空响应提示', () => {
  assert.equal(shouldShowEmptyAssistantResponse('complete'), true)
  assert.equal(shouldShowEmptyAssistantResponse('streaming'), false)
  assert.equal(shouldShowEmptyAssistantResponse('cancelled'), false)
  assert.equal(shouldShowEmptyAssistantResponse('error'), false)
})
