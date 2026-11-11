import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Merchant service watch browser checks run only in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT)
assert.ok(process.env.RUNNER_TEMP)
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const zh = JSON.parse(await readFile(resolve(directory, 'src/i18n/merchant-pool.json'), 'utf8')).zh
const watchZh = JSON.parse(await readFile(resolve(directory, 'src/i18n/merchant-service-watch.json'), 'utf8')).zh
const label = key => key.split('.').reduce((value, part) => value[part], zh)
const watchLabel = key => key.split('.').reduce((value, part) => value[part], watchZh)
const html = resolve(directory, '.merchant-service-watch-smoke.html')
const entry = resolve(directory, '.merchant-service-watch-smoke.tsx')
const origin = 'http://127.0.0.1:4183'
const now = Math.floor(Date.now() / 1000)
const pool = { id: 1, group_id: 8, name: 'Watch smoke pool', description: 'Public test pool', enabled: true, commission_bps: 500, effective_commission_bps: 500, commission_inherit_global: false, min_ratio_micros: 100000, max_ratio_micros: 10000000, settlement_delay_secs: 86400, created_at: now, updated_at: now }
const offer = { model: 'watch-fixture', provider: 'DeepSeek', sales_ratio_micros: 800000, context_window: 8192, max_output_tokens: 1024, supports_streaming: true, supports_tools: true, supports_vision: false, prices: { currency: 'USD', unit: 'million_tokens', billing_mode: 'token', video_per_second: null, input: '0.80', output: '1.60', cache_read: '0.08', cache_write_5m: '1.00', cache_write_1h: '1.60', version: 1 } }
const service = { id: 51, pool_id: 1, merchant_id: 6, merchant_name: 'Watch merchant', name: 'Watch service', description: 'Published service', status: 'published', revision: 1, price_version: 1, protocol: 'openai_chat', maintenance: [], models: [offer], updated_at: now }
let watches = []
const watchWrites = []
const errors = []
let server, browser, page, serverOutput = ''

try {
  await mkdir(process.env.RUNNER_TEMP, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.merchant-service-watch-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter, Route, Routes } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import './src/index.css'; import './src/i18n';
    import { useSiteTheme } from './src/shared/components/use-site-theme';
    import { MerchantPoolPage } from './src/features/merchant-pool/merchant-pool-page';
    import { MerchantDetailPage } from './src/features/merchant-pool/merchant-detail-page';
    import { useNotifications } from './src/shared/nav/hooks/use-notifications';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function Notifications() { const query = useNotifications(); return <section aria-label="Notification smoke">{query.notifications.map(item => <article key={item.id}><h2>{item.title}</h2><p>{item.description}</p></article>)}</section> }
    function App() { useSiteTheme(); const [view, setView] = useState('market'); return <HeroUIProvider><QueryClientProvider client={client}><main style={{ padding: 16, maxWidth: 1200, margin: 'auto' }}><nav aria-label="Smoke views"><button onClick={() => setView('market')}>Market</button><button onClick={() => setView('notifications')}>Notifications</button></nav>{view === 'market' ? <Routes><Route path="/console/merchant-pool/merchants/:id" element={<MerchantDetailPage />} /><Route path="*" element={<MerchantPoolPage />} /></Routes> : <Notifications />}</main></QueryClientProvider></HeroUIProvider> }
    createRoot(document.getElementById('root')).render(<BrowserRouter><App /></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', '4183', '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let i = 0; i < 60; i += 1) { try { if ((await fetch(`${origin}/.merchant-service-watch-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 950 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); localStorage.setItem('anyflows.theme', 'light'); sessionStorage.setItem('anyflows.management.access-token', 'ci-merchant-watch') })
  await page.route(url => url.pathname.startsWith('/api/'), async intercepted => {
    const request = intercepted.request(), path = new URL(request.url()).pathname, method = request.method()
    const respond = (body, status = 200) => intercepted.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    const assertAuthenticated = () => assert.equal(request.headers().authorization, 'Bearer ci-merchant-watch')
    if (path === '/api/site') return respond({ balance_display: { mode: 'quota', unit_name: '积分', unit_symbol: '积分', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } })
    if (path === '/api/merchant-pools/1/model-providers') return respond({ providers: [{ name: 'DeepSeek', model_count: 1 }] })
    if (path === '/api/merchant-pools' && method === 'GET') return respond({ items: [pool], total: 1, page: 1, page_size: 20 })
    if (path === '/api/merchant-pools/1/services' && method === 'GET') return respond({ items: [service], total: 1, page: 1, page_size: 20 })
    if (path === '/api/merchant-pools/1/services/51' && method === 'GET') return respond(service)
    if (path === '/api/merchant-pool/watches' && method === 'GET') { assertAuthenticated(); return respond({ items: watches, total: watches.length }) }
    if (path === '/api/merchant-pools/1/services/51/watch' && method === 'PUT') {
      assertAuthenticated()
      watchWrites.push('put')
      watches = [{ pool_id: 1, pool_name: pool.name, service_id: 51, merchant_id: 6, service_name: service.name, merchant_name: service.merchant_name, active: true, created_at: now }]
      return respond(watches[0])
    }
    if (path === '/api/merchant-pools/1/services/51/watch' && method === 'DELETE') {
      assertAuthenticated()
      watchWrites.push('delete')
      watches = []
      return respond({ changed: true })
    }
    if (path === '/api/merchant-pools/1/statistics') return respond({ items: [], as_of: now, window_seconds: 86400 })
    if (path === '/api/merchant-pools/1/timeline') return respond({ items: [], as_of: now, window_seconds: 86400 })
    if (path === '/api/account/notifications' && method === 'GET') { assertAuthenticated(); return respond({ entries: [{ id: 901, kind: 'product_update', channel: 'in_app', template_version: 'v1', occurred_at: now, delivery_state: 'available', delivery_attempts: 0, read_at: null, merchant_service_event_id: 77, merchant_service_event_kind: 'price_changed', merchant_service_pool_id: 1, merchant_service_id: 51, merchant_service_merchant_id: 6, merchant_service_name: service.name }], next_cursor: null, unread_count: 1 }) }
    return respond({ error: { code: 'not_found' } }, 404)
  })
  await page.goto(`${origin}/.merchant-service-watch-smoke.html`)
  await page.getByRole('heading', { name: label('market.title'), exact: true }).waitFor()
  const serviceRow = page.getByRole('region', { name: label('market.catalogTable'), exact: true }).getByRole('row').filter({ hasText: service.name })
  await serviceRow.getByRole('link', { name: service.models[0].model, exact: true }).click()
  await page.getByRole('heading', { name: service.name, exact: true }).waitFor()
  const watch = page.getByRole('button', { name: label('market.watch'), exact: true })
  await watch.waitFor()
  const watched = page.waitForResponse(response => new URL(response.url()).pathname === '/api/merchant-pools/1/services/51/watch' && response.request().method() === 'PUT')
  await watch.click()
  await watched
  await page.getByRole('button', { name: label('market.unwatch'), exact: true }).waitFor()
  assert.equal(watchWrites.join(','), 'put', 'Following a public service persists through the authenticated watch endpoint')
  const unwatched = page.waitForResponse(response => new URL(response.url()).pathname === '/api/merchant-pools/1/services/51/watch' && response.request().method() === 'DELETE')
  await page.getByRole('button', { name: label('market.unwatch'), exact: true }).click()
  await unwatched
  await page.getByRole('button', { name: label('market.watch'), exact: true }).waitFor()
  assert.equal(watchWrites.join(','), 'put,delete', 'A user can stop future notifications without deleting notification history')
  await page.getByRole('button', { name: 'Notifications', exact: true }).click()
  const notifications = page.getByRole('region', { name: 'Notification smoke', exact: true })
  await notifications.getByRole('heading', { name: watchLabel('profile.notificationHistory.kind.merchant_service_update'), exact: true }).waitFor()
  await notifications.getByText(watchLabel('profile.notificationHistory.merchantServiceEvent.price_changed').replace('{{service}}', service.name), { exact: true }).waitFor()
  assert.deepEqual(errors, [])
  console.log('Merchant service watch browser regression passed')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(process.env.RUNNER_TEMP, 'merchant-service-watch-failure.png'), fullPage: true }).catch(() => {})
  console.error(JSON.stringify({ error: String(error), errors, serverOutput }, null, 2))
  throw error
} finally {
  await browser?.close()
  server?.kill('SIGTERM')
}
