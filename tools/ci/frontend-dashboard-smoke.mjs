import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { writeFile, unlink } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const frontend = process.argv[2]
assert.ok(['web', 'web-next'].includes(frontend))
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Browser tests run in remote CI')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const directory = resolve(root, frontend)
const isNext = frontend === 'web-next'
const port = isNext ? 4174 : 4173
const html = resolve(directory, '.dashboard-smoke.html')
const entry = resolve(directory, '.dashboard-smoke.tsx')
const errors = []
let browser
let server
let serverOutput = ''
const start = 1_790_812_800
const hourly = Array.from({ length: 24 }, (_, index) => ({
  period_start: start + index * 3600, period_end: start + (index + 1) * 3600,
  request_count: [32, 23, 18, 12, 0, 9, 24, 66, 105, 138, 184, 156, 126, 164, 210, 192, 178, 150, 186, 242, 206, 132, 88, 64][index],
  quota_consumed: (index + 1) * 1357,
}))
const total = hourly.reduce((sum, point) => sum + point.request_count, 0)
const quota = hourly.reduce((sum, point) => sum + point.quota_consumed, 0)
const dashboard = {
  period_start: start, period_end: start + 86400, request_count: total,
  quota_consumed: quota, upstream_usage_count: total - 50, estimated_usage_count: 50,
  per_token_request_count: total - 20, per_call_request_count: 15, free_request_count: 5,
  enabled_channel_count: 8, disabled_channel_count: 1, auto_disabled_channel_count: 1,
  outcome_request_count: total + 20, successful_request_count: total,
  failed_request_count: 20, other_success_count: 0,
  failures: [{ kind: 'upstream_network', request_count: 14 }, { kind: 'outcome_unknown', request_count: 6 }],
  channel_flows: [{ protocol: 'openai_chat', channel_id: 1, channel_name: 'OpenAI Production', request_count: total }],
  flow_request_count: total, flow_quota_consumed: quota,
  flow_paths: [{ user_id: 1, group_id: 1, group_name: 'Default', channel_id: 1, channel_name: 'OpenAI Production', model: 'gpt-5', request_count: total, quota_consumed: quota }],
  hourly,
  performance: { first_token_sample_count: total, average_first_token_ms: 428, slow_first_token_count: 12, slow_first_token_threshold_ms: 2000, duration_sample_count: total, average_duration_ms: 1840, slow_request_count: 20, slow_request_threshold_ms: 10000 },
}
const models = ['gpt-5', 'deepseek-flash', 'qwen3-max', 'claude-sonnet-4.5', 'gemini-2.5-pro', 'unknown-only', ...Array.from({ length: 19 }, (_, i) => `custom-model-${i + 1}`)]
const channels = ['OpenAI Production', 'DeepSeek Asia', 'Qwen Primary', 'Anthropic Backup', 'Gemini Global', 'unknown-only', ...Array.from({ length: 19 }, (_, i) => `Channel ${i + 1}`)]
function row(name, index) {
  const points = Array.from({ length: 24 }, (_, hour) => ({
    period_start: start + hour * 3600,
    successful_request_count: index === 5 || hour === 4 ? 0 : 100 + hour * 13,
    failed_request_count: index === 5 ? 0 : (hour === 9 ? index * 3 : 0),
    unknown_request_count: index === 5 ? 1 : (hour === 18 ? 2 : 0),
  }))
  const successful = points.reduce((sum, point) => sum + point.successful_request_count, 0)
  const failed = points.reduce((sum, point) => sum + point.failed_request_count, 0)
  const unknown = points.reduce((sum, point) => sum + point.unknown_request_count, 0)
  return { key: String(index + 1), name, request_count: successful + failed + unknown, successful_request_count: successful, failed_request_count: failed, unknown_request_count: unknown, average_duration_ms: index === 5 ? null : 820 + index * 410, hourly: points }
}

try {
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Dashboard CI</title><div id="root"></div><script type="module" src="/.dashboard-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { ${isNext ? 'AdminDashboardView' : 'DashboardPage'} as Dashboard } from './src/features/dashboard/dashboard-page';
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '20px', maxWidth: '1240px', margin: 'auto' }}><Dashboard /></main>${isNext ? '</HeroUIProvider>' : ''}
      </QueryClientProvider>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.dashboard-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => localStorage.setItem('anyflows.locale', 'zh'))
  const requests = []
  let failSla = false
  let shrink = false
  let failureOnly = false
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const url = new URL(route.request().url())
    let response
    if (url.pathname === '/api/admin/dashboard') {
      response = failureOnly ? {
        ...dashboard, request_count: 0, quota_consumed: 0, upstream_usage_count: 0, estimated_usage_count: 0,
        per_token_request_count: 0, per_call_request_count: 0, free_request_count: 0,
        enabled_channel_count: 0, disabled_channel_count: 0, auto_disabled_channel_count: 0,
        outcome_request_count: 20, successful_request_count: 0,
        channel_flows: [], flow_request_count: 0, flow_quota_consumed: 0, flow_paths: [],
        hourly: hourly.map(point => ({ ...point, request_count: 0, quota_consumed: 0 })),
        performance: { ...dashboard.performance, first_token_sample_count: 0, average_first_token_ms: null, slow_first_token_count: 0, duration_sample_count: 0, average_duration_ms: null, slow_request_count: 0 },
      } : dashboard
    } else if (url.pathname === '/api/site') {
      response = { balance_display: { mode: 'quota', unit_name: '', unit_symbol: '', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } }
    } else if (url.pathname === '/api/admin/analytics/export-status') {
      response = { enabled: false, state: 'disabled', backlog_count: 0 }
    } else if (url.pathname === '/api/admin/dashboard/service-levels') {
      requests.push(Object.fromEntries(url.searchParams))
      if (failSla) { await route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ code: 'internal_error' }) }); return }
      const names = url.searchParams.get('dimension') === 'channel' ? channels : models
      let items = names.map(row).filter(item => item.name.toLowerCase().includes((url.searchParams.get('search') ?? '').toLowerCase()))
      if (shrink) items = items.slice(0, 1)
      if (url.searchParams.get('sort') === 'failures') items.sort((a, b) => b.failed_request_count - a.failed_request_count)
      const size = Number(url.searchParams.get('page_size'))
      const offset = (Number(url.searchParams.get('page')) - 1) * size
      response = { period_start: start, period_end: start + 86400, total: items.length, unattributed_request_count: 7, items: items.slice(offset, offset + size) }
    } else { errors.push(`Unexpected API: ${url.pathname}`); await route.fulfill({ status: 404, body: '{}' }); return }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(response) })
  })
  await page.goto(`http://127.0.0.1:${port}/.dashboard-smoke.html`)
  await page.locator('.report-sla-row').first().waitFor()
  const sla = page.locator('.report-sla')
  assert.equal(await sla.locator('.report-sla-row').count(), 10)
  assert.equal(await page.locator('.report-chart-hitareas button').count(), 24)
  assert.ok((await page.locator('.report-chart-canvas path').first().getAttribute('d')).length > 100)
  await page.locator('.report-chart-hitareas button').nth(23).focus()
  await page.keyboard.press('ArrowLeft')
  assert.equal(await page.locator('.report-chart-hitareas button').nth(22).evaluate(node => node === document.activeElement), true)
  await page.locator('.report-segments').first().getByRole('button').nth(1).click()
  await page.waitForFunction(() => document.querySelector('.report-chart-readout')?.textContent?.includes('额度'))
  await page.locator('.report-segments').first().getByRole('button').nth(0).click()
  const health = sla.locator('.report-health-grid').first()
  await health.locator('button').last().focus()
  await page.keyboard.press('ArrowLeft')
  assert.equal(await health.locator('button').nth(22).evaluate(node => node === document.activeElement), true)
  await sla.getByRole('button', { name: '下一页', exact: true }).click()
  await sla.getByText('custom-model-5', { exact: true }).waitFor()
  assert.equal(requests.at(-1).page, '2')
  await sla.getByRole('combobox', { name: '每页', exact: true }).selectOption('5')
  await sla.getByText('gpt-5', { exact: true }).waitFor()
  assert.equal(await sla.locator('.report-sla-row').count(), 5)
  await sla.getByRole('textbox', { name: '搜索模型或渠道' }).fill('unknown-only')
  await sla.getByText('unknown-only', { exact: true }).waitFor()
  assert.equal(await sla.locator('.report-sla-rate strong').innerText(), '--')
  await sla.getByRole('textbox').fill('no-match')
  await sla.getByText('未找到匹配的调用记录', { exact: true }).waitFor()
  await sla.getByRole('textbox').fill('')
  await sla.getByText('gpt-5', { exact: true }).waitFor()
  await sla.getByRole('button', { name: '渠道', exact: true }).click()
  await sla.getByText('OpenAI Production', { exact: true }).waitFor()
  assert.equal(requests.at(-1).dimension, 'channel')
  await sla.getByRole('combobox', { name: '排序', exact: true }).selectOption('failures')
  await page.waitForFunction(() => document.querySelector('.report-sla-name strong')?.textContent === 'Channel 19')
  await sla.getByRole('combobox', { name: '目标', exact: true }).selectOption('0.99')
  const requestCount = requests.length
  await sla.getByRole('combobox', { name: '目标', exact: true }).selectOption('0.9999')
  assert.equal(requests.length, requestCount, 'Target changes use the loaded outcomes')
  await sla.getByRole('combobox', { name: '排序', exact: true }).selectOption('requests')
  await sla.getByText('OpenAI Production', { exact: true }).waitFor()
  await sla.getByRole('button', { name: '下一页', exact: true }).click()
  await sla.getByText('unknown-only', { exact: true }).waitFor()
  shrink = true
  await sla.getByRole('button', { name: '刷新', exact: true }).click()
  await sla.getByText('第 1 / 1 页 · 共 1 项', { exact: true }).waitFor()
  assert.equal(requests.at(-1).page, '1', 'Shrinking results clamp pagination')
  shrink = false
  failSla = true
  await sla.getByRole('button', { name: '刷新', exact: true }).click()
  await sla.getByRole('alert').waitFor()
  failSla = false
  await sla.getByRole('button', { name: '重试', exact: true }).click()
  await sla.getByText('OpenAI Production', { exact: true }).waitFor()

  for (const theme of ['dark', 'light']) {
    await page.evaluate(mode => { document.documentElement.classList.remove('dark', 'light'); document.documentElement.classList.add(mode) }, theme)
    for (const [name, width, height] of [['desktop', 1280, 960], ['tablet', 768, 1024], ['mobile', 390, 844], ['small-mobile', 320, 740]]) {
      await page.setViewportSize({ width, height })
      await page.evaluate(() => document.fonts.ready)
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${frontend} ${theme} overflow at ${width}px`)
      const visibleOverlap = await sla.evaluate(node => {
        const parts = [...node.querySelector('.report-sla-row').children].map(element => element.getBoundingClientRect()).filter(rect => rect.width && rect.height)
        return parts.some((a, index) => parts.slice(index + 1).some(b => a.left < b.right - 1 && b.left < a.right - 1 && a.top < b.bottom - 1 && b.top < a.bottom - 1))
      })
      assert.equal(visibleOverlap, false, `SLA columns overlap at ${width}px`)
      if (name === 'desktop' || name === 'mobile') {
        const screenshot = await page.screenshot({ fullPage: true, animations: 'disabled' })
        console.log(`ANYFLOWS_REPORT_${frontend}_${theme}_${name}_BEGIN`)
        console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
        console.log(`ANYFLOWS_REPORT_${frontend}_${theme}_${name}_END`)
      }
    }
  }
  failureOnly = true
  await page.reload()
  await page.locator('.report-sla-row').first().waitFor()
  assert.equal(await page.locator('.report-chart-hitareas button').count(), 24, 'Failure-only windows must remain visible')
  assert.deepEqual(errors, [])
  console.log(`Dashboard browser checks passed for ${frontend}: charts, keyboard, unknown outcomes, filters, pagination, recovery, target changes, failure-only windows and responsive themes.`)
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
