import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Channel monitor browser checks run only in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT)
assert.ok(process.env.RUNNER_TEMP)
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const zh = JSON.parse(await readFile(resolve(directory, 'src/i18n/locales/zh.json'), 'utf8'))
const label = key => {
  const value = key.split('.').reduce((node, part) => node?.[part], zh)
  assert.equal(typeof value, 'string', `Missing Chinese translation: ${key}`)
  return value
}
const html = resolve(directory, '.channel-monitor-smoke.html')
const entry = resolve(directory, '.channel-monitor-smoke.tsx')
const origin = 'http://127.0.0.1:4184'
const now = Math.floor(Date.now() / 1000)
const target = {
  id: 71, channel_id: 11, credential_id: 12, model: 'chat-fixture', enabled: false,
  interval_secs: 300, daily_budget: 12, timeout_secs: 15, alert_after_secs: 900,
  daily_used: 0, last_attempt_epoch_seconds: now - 600, last_success_epoch_seconds: now - 900,
  failure_since_epoch_seconds: null, alert_active: false, muted_until_epoch_seconds: null,
}
const channels = [{ id: 11, name: 'CI channel', type: 'openai', status: 'enabled', protocol: 'openai_chat', models: ['chat-fixture'] }]
const credentials = [{ id: 12, kind: 'api_key', status: 'enabled', schedulable: true, oauth_token_pending: false }]
const events = [
  { id: 101, target_id: 71, kind: 'down', latency_ms: null, occurred_epoch_seconds: now - 1200 },
  { id: 102, target_id: 71, kind: 'recovered', latency_ms: 8, occurred_epoch_seconds: now - 900 },
]
const requests = []
const targetWrites = []
const errors = []
let server, browser, page, serverOutput = ''

try {
  await mkdir(process.env.RUNNER_TEMP, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.channel-monitor-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import './src/index.css'; import './src/i18n';
    import { useSiteTheme } from './src/shared/components/use-site-theme';
    import { ActiveChannelMonitorPanel } from './src/features/channels/active-channel-monitor-panel';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function App() { useSiteTheme(); return <HeroUIProvider><QueryClientProvider client={client}><main style={{ padding: 16, maxWidth: 960, margin: 'auto' }}><ActiveChannelMonitorPanel /></main></QueryClientProvider></HeroUIProvider> }
    createRoot(document.getElementById('root')).render(<BrowserRouter><App /></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', '4184', '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) { try { if ((await fetch(`${origin}/.channel-monitor-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 900 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    localStorage.setItem('anyflows.theme', 'light')
    sessionStorage.setItem('anyflows.management.access-token', 'ci-channel-monitor')
  })
  await page.route(url => url.pathname.startsWith('/api/'), async intercepted => {
    const request = intercepted.request()
    const path = new URL(request.url()).pathname
    const method = request.method()
    requests.push({ path, method })
    assert.equal(request.headers().authorization, 'Bearer ci-channel-monitor')
    const respond = (body, status = 200) => intercepted.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    if (path === '/api/admin/channel-monitor/settings' && method === 'GET') return respond({ enabled: false, global_daily_budget: 288 })
    if (path === '/api/admin/channel-monitor/settings' && method === 'PATCH') return respond(request.postDataJSON())
    if (path === '/api/admin/channel-monitor/targets' && method === 'GET') return respond([target])
    if (path === '/api/admin/channel-monitor/targets' && method === 'POST') {
      targetWrites.push(request.postDataJSON())
      return respond({ ...target, ...request.postDataJSON(), id: 72 })
    }
    if (path === '/api/admin/channel-monitor/targets/71/events' && method === 'GET') return respond(events)
    if (path === '/api/admin/channels' && method === 'GET') return respond({ channels, next_cursor: null })
    if (path === '/api/admin/channels/11/credentials' && method === 'GET') return respond({ credentials, next_cursor: null })
    errors.push(`Unexpected request: ${method} ${path}`)
    return respond({ error: { code: 'not_found' } }, 404)
  })

  await page.goto(`${origin}/.channel-monitor-smoke.html`)
  await page.getByText(label('channels.activeMonitor.title'), { exact: true }).waitFor()
  const details = page.locator('details').filter({ hasText: label('channels.activeMonitor.title') })
  await details.locator('summary').click()
  await page.getByText(label('channels.activeMonitor.alerting'), { exact: true }).waitFor({ state: 'detached' })
  await page.getByText(label('channels.activeMonitor.targetDisabledStatus'), { exact: true }).waitFor()
  await page.getByText(new RegExp(label('channels.activeMonitor.lastProbe').split('{{value}}')[0])).waitFor()
  await page.getByText(new RegExp(label('channels.activeMonitor.lastSuccess').split('{{value}}')[0])).waitFor()

  const globalSwitch = page.getByRole('switch', { name: label('channels.activeMonitor.globalEnabled') })
  await globalSwitch.waitFor()
  assert.equal(await globalSwitch.isChecked(), false, 'Global monitoring starts disabled')

  await page.getByRole('button', { name: label('channels.activeMonitor.showEvents') }).click()
  const eventEntries = details.locator('div.mt-2.flex.flex-wrap.gap-2 > span')
  await eventEntries.filter({ hasText: label('channels.activeMonitor.events.down') }).waitFor()
  await eventEntries.filter({ hasText: label('channels.activeMonitor.events.recovered') }).waitFor()

  await details.locator('fieldset label').filter({ hasText: label('channels.activeMonitor.channel') }).locator('select').selectOption('11')
  await details.locator('fieldset label').filter({ hasText: label('channels.activeMonitor.credential') }).locator('select').selectOption('12')
  await details.locator('fieldset label').filter({ hasText: label('channels.activeMonitor.model') }).locator('select').selectOption('chat-fixture')
  await page.getByLabel(label('channels.activeMonitor.interval'), { exact: true }).fill('300')
  await page.getByLabel(label('channels.activeMonitor.budget'), { exact: true }).fill('1')
  await page.getByLabel(label('channels.activeMonitor.timeout'), { exact: true }).fill('1')
  await page.getByLabel(label('channels.activeMonitor.alertAfter'), { exact: true }).fill('60')
  const saveTarget = page.getByRole('button', { name: label('channels.activeMonitor.saveTarget') })
  await saveTarget.waitFor()
  assert.equal(await saveTarget.isDisabled(), false, 'Minimum accepted target limits remain saveable')
  const saved = page.waitForResponse(response => new URL(response.url()).pathname === '/api/admin/channel-monitor/targets' && response.request().method() === 'POST')
  await saveTarget.click()
  await saved
  assert.deepEqual(targetWrites[0], {
    channel_id: 11, credential_id: 12, model: 'chat-fixture', enabled: false,
    interval_secs: 300, daily_budget: 1, timeout_secs: 1, alert_after_secs: 60,
    muted_until_epoch_seconds: null,
  })

  assert.deepEqual(errors, [])
  assert.ok(requests.every(({ path }) => path.startsWith('/api/admin/channel-monitor/') || path === '/api/admin/channels' || path.startsWith('/api/admin/channels/')))
  console.log('Channel monitor browser regression passed; all API calls were mocked')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(process.env.RUNNER_TEMP, 'channel-monitor-failure.png'), fullPage: true }).catch(() => {})
  console.error(JSON.stringify({ error: String(error), errors, requests, serverOutput }, null, 2))
  throw error
} finally {
  await browser?.close()
  server?.kill('SIGTERM')
}
