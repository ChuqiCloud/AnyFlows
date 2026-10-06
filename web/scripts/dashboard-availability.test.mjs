import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const sourceUrl = new URL('../src/features/dashboard/dashboard-availability-metrics.ts', import.meta.url)
const source = await readFile(sourceUrl, 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`
const { getDashboardAvailabilityMetrics } = await import(moduleUrl)

function dashboard({ success, failed, unknown }) {
  return {
    successful_request_count: success,
    failed_request_count: failed,
    outcome_request_count: success + failed,
    failures: unknown === 0 ? [] : [{ kind: 'outcome_unknown', request_count: unknown }],
  }
}

test('可用性只用可判定终态计算成功率并单列结果未知', () => {
  assert.deepEqual(
    getDashboardAvailabilityMetrics(dashboard({ success: 8, failed: 4, unknown: 2 })),
    {
      unknownRequestCount: 2,
      confirmedFailureCount: 2,
      resolvedRequestCount: 10,
      successRate: 0.8,
      failureRate: 0.2,
      resolutionRate: 10 / 12,
    },
  )
})

test('全未知和无样本都保持不可计算成功率', () => {
  const unknownOnly = getDashboardAvailabilityMetrics(dashboard({ success: 0, failed: 3, unknown: 3 }))
  assert.equal(unknownOnly.successRate, null)
  assert.equal(unknownOnly.failureRate, null)
  assert.equal(unknownOnly.resolutionRate, 0)

  const empty = getDashboardAvailabilityMetrics(dashboard({ success: 0, failed: 0, unknown: 0 }))
  assert.equal(empty.successRate, null)
  assert.equal(empty.failureRate, null)
  assert.equal(empty.resolutionRate, null)
})
