import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const sourceUrl = new URL('../src/features/dashboard/dashboard-performance-metrics.ts', import.meta.url)
const source = await readFile(sourceUrl, 'utf8')
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ESNext,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`
const { getDashboardPerformanceMetrics } = await import(moduleUrl)

function dashboard({ requests, firstTokenSamples, slowFirstToken, durationSamples, slowDuration }) {
  return {
    request_count: requests,
    performance: {
      first_token_sample_count: firstTokenSamples,
      slow_first_token_count: slowFirstToken,
      duration_sample_count: durationSamples,
      slow_request_count: slowDuration,
    },
  }
}

test('性能样本拆分阈值内、慢请求和未采样三类事实', () => {
  const metrics = getDashboardPerformanceMetrics(dashboard({
    requests: 10,
    firstTokenSamples: 8,
    slowFirstToken: 2,
    durationSamples: 5,
    slowDuration: 1,
  }))

  assert.deepEqual(metrics.firstToken, {
    sampleCount: 8,
    slowCount: 2,
    belowThresholdCount: 6,
    unsampledCount: 2,
    sampleCoverage: 0.8,
    belowThresholdRate: 0.75,
    belowThresholdShare: 0.6,
    slowShare: 0.2,
  })
  assert.equal(metrics.duration.sampleCoverage, 0.5)
  assert.equal(metrics.duration.belowThresholdRate, 0.8)
})

test('无请求或无耗时样本时不伪造百分比', () => {
  const empty = getDashboardPerformanceMetrics(dashboard({
    requests: 0,
    firstTokenSamples: 0,
    slowFirstToken: 0,
    durationSamples: 0,
    slowDuration: 0,
  }))
  assert.equal(empty.firstToken.sampleCoverage, null)
  assert.equal(empty.firstToken.belowThresholdRate, null)

  const unsampled = getDashboardPerformanceMetrics(dashboard({
    requests: 4,
    firstTokenSamples: 0,
    slowFirstToken: 0,
    durationSamples: 0,
    slowDuration: 0,
  }))
  assert.equal(unsampled.duration.sampleCoverage, 0)
  assert.equal(unsampled.duration.belowThresholdRate, null)
  assert.equal(unsampled.duration.unsampledCount, 4)
})
