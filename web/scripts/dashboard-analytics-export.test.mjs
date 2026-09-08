import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const [openapi, apiSource, pageSource, zh, en] = await Promise.all([
  readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
  readFile(new URL('../src/features/dashboard/dashboard-analytics-export-api.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/dashboard/dashboard-analytics-export.tsx', import.meta.url), 'utf8'),
  readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
  readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
])

test('分析导出运维入口保持有界状态与重放契约', () => {
  assert.ok(openapi.paths['/api/admin/analytics/export-status'])
  assert.ok(openapi.paths['/api/admin/analytics/export-replay'])
  assert.match(apiSource, /replayAdminAnalyticsExport/)
  assert.match(pageSource, /status === 'backlog'/)
  assert.match(pageSource, /replay\.mutate\(64\)/)
  for (const locale of [zh, en]) {
    assert.ok(locale.dashboard.analyticsExport.title)
    assert.ok(locale.dashboard.analyticsExport.states.unavailable)
    assert.ok(!Object.values(locale.dashboard.analyticsExport).some((value) => value === '?'))
  }
})
