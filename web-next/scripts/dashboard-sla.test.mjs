import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

const source = await readFile(new URL('../src/features/dashboard/dashboard-sla-model.ts', import.meta.url), 'utf8')
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { serviceLevelMetrics, serviceLevelTone } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`)
const counts = (success, failed, unknown = 0) => ({ successful_request_count: success, failed_request_count: failed, unknown_request_count: unknown })

test('未知结果不降低成功率，但降低样本覆盖率', () => {
  const metrics = serviceLevelMetrics(counts(990, 10, 100), 0.99)
  assert.equal(metrics.rate, 0.99)
  assert.equal(metrics.coverage, 1000 / 1100)
  assert.equal(metrics.state, 'healthy')
  assert.equal(metrics.remainingFailures, 0)
  assert.equal(metrics.exceededFailures, 0)
})

test('99.9% 目标按整数样本计算失败预算，边界不受浮点误差影响', () => {
  assert.equal(serviceLevelMetrics(counts(999, 1), 0.999).state, 'healthy')
  assert.equal(serviceLevelMetrics(counts(999, 1), 0.999).remainingFailures, 0)
  assert.equal(serviceLevelMetrics(counts(998, 2), 0.999).exceededFailures, 1)
  assert.equal(serviceLevelMetrics(counts(1000, 0), 0.999).remainingFailures, 1)
  assert.equal(serviceLevelMetrics(counts(9995, 5), 0.9995).state, 'healthy')
  assert.equal(serviceLevelMetrics(counts(9999, 1), 0.9999).exceededFailures, 0)
})

test('空样本和全部未知都不显示虚假的 100%，健康格分辨无样本与未知', () => {
  assert.equal(serviceLevelMetrics(counts(0, 0), 0.999).rate, null)
  assert.equal(serviceLevelMetrics(counts(0, 0, 5), 0.999).rate, null)
  assert.equal(serviceLevelMetrics(counts(0, 0, 5), 0.999).coverage, 0)
  assert.equal(serviceLevelTone(counts(0, 0), 0.999), 'empty')
  assert.equal(serviceLevelTone(counts(0, 0, 5), 0.999), 'unknown')
  assert.equal(serviceLevelTone(counts(100, 0), 0.999), 'healthy')
  assert.equal(serviceLevelTone(counts(98, 2), 0.999), 'degraded')
  assert.equal(serviceLevelTone(counts(0, 5), 0.999), 'critical')
})
