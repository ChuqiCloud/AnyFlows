import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Merchant pool browser checks run only in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT)
assert.ok(process.env.RUNNER_TEMP)
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const copy = JSON.parse(await readFile(resolve(directory, 'src/i18n/merchant-pool.json'), 'utf8')).zh
const commonCopy = JSON.parse(await readFile(resolve(directory, 'src/i18n/locales/zh.json'), 'utf8'))
const label = key => key.split('.').reduce((value, part) => value[part], copy)
const commonLabel = key => key.split('.').reduce((value, part) => value[part], commonCopy)
const html = resolve(directory, '.merchant-pool-smoke.html')
const entry = resolve(directory, '.merchant-pool-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'merchant-pool-screenshots-web-next')
const port = 4181
const origin = `http://127.0.0.1:${port}`
const now = Math.floor(Date.now() / 1000)
const pool = { id: 1, group_id: 8, name: 'CI shared pool', description: 'CI pool', enabled: true, commission_bps: 500, effective_commission_bps: 500, commission_inherit_global: false, min_ratio_micros: 100000, max_ratio_micros: 10000000, settlement_delay_secs: 86400, created_at: now, updated_at: now }
const offer = { model: 'gpt-fixture', sales_ratio_micros: 800000, context_window: 128000, max_output_tokens: 4096, supports_streaming: true, supports_tools: true, supports_vision: false, prices: { currency: 'USD', unit: 'million_tokens', input: '0.80', output: '1.60', cache_read: '0.08', cache_write_5m: '1.00', cache_write_1h: '1.60', version: 1 } }
const publicService = { id: 51, pool_id: 1, merchant_id: 6, merchant_name: 'CI merchant', name: 'CI service', description: 'Reviewed service', status: 'published', revision: 1, models: [offer], updated_at: now }
const excludedService = { ...publicService, id: 52, merchant_id: 7, merchant_name: 'CI excluded merchant', name: 'CI excluded service' }
const profile = { id: 6, owner_user_id: 3, name: 'CI merchant', description: 'CI relay', status: 'approved', review_reason: null, created_at: now, updated_at: now }
const list = items => ({ items, total: items.length, page: 1, page_size: 20 })
let routes = []
let services = []
let routeFailures = 1
let previewFailures = 1
let withdrawalFailures = 1
const routeRequests = []
const previewRequests = []
const withdrawalRequests = []
const errors = []
let server, browser, page
let serverOutput = ''

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.merchant-pool-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import './src/index.css'; import './src/i18n';
    import { MerchantPoolPage } from './src/features/merchant-pool/merchant-pool-page';
    import { MerchantPoolRoutesPage } from './src/features/merchant-pool/merchant-pool-routes-page';
    import { MerchantWorkspacePage } from './src/features/merchant-pool/merchant-workspace-page';
    import { ApiKeyForm } from './src/features/api-keys/api-key-form';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function App() { const [view, setView] = useState('catalog'); const [issued, setIssued] = useState(null); return <HeroUIProvider><QueryClientProvider client={client}><main style={{ padding: 16, maxWidth: 1200, margin: 'auto' }}><nav aria-label="CI views">{['catalog','routes','workspace','key'].map(value => <button key={value} onClick={() => setView(value)}>{value}</button>)}</nav>{view === 'catalog' ? <MerchantPoolPage /> : view === 'routes' ? <MerchantPoolRoutesPage /> : view === 'workspace' ? <MerchantWorkspacePage /> : <ApiKeyForm initialRouteId={21} onCancel={() => setView('routes')} onSaved={() => setView('routes')} onIssued={setIssued} />}{issued ? <output data-testid="issued">issued</output> : null}</main></QueryClientProvider></HeroUIProvider> }
    createRoot(document.getElementById('root')).render(<BrowserRouter><App /></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let i = 0; i < 60; i += 1) { try { if ((await fetch(`${origin}/.merchant-pool-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 1000 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); sessionStorage.setItem('anyflows.management.access-token', 'ci-merchant') })
  await page.route(url => url.pathname.startsWith('/api/'), async intercepted => {
    const request = intercepted.request(), path = new URL(request.url()).pathname, method = request.method()
    const respond = (body, status = 200) => intercepted.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    if (path === '/api/site') return respond({ balance_display: { mode: 'quota', unit_name: '积分', unit_symbol: '积分', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } })
    assert.equal(request.headers().authorization, 'Bearer ci-merchant')
    if (path === '/api/merchant-pools' || path === '/api/merchant/available-pools') return respond(list([pool]))
    if (path === '/api/merchant-pools/1/services') return respond(list(new URL(request.url()).searchParams.get('model') ? [publicService, excludedService, { ...publicService, id: 53, merchant_id: 8, merchant_name: 'CI reserve merchant', name: 'CI reserve service' }] : [publicService, excludedService]))
    if (path === '/api/merchant-pools/1/statistics') return respond({ items: [{ service_id: 51, merchant_id: 6, sample_count: 10, success_count: 9, failure_count: 1, success_rate: 90, ttft_p50_ms: 123, ttft_p95_ms: 400, latency_p50_ms: 300, latency_p95_ms: 900, status: 'normal', updated_at: now }], as_of: now, window_seconds: 86400 })
    if (path === '/api/merchant-pools/1/timeline') return respond({ items: [], as_of: now, window_seconds: 86400 })
    if (path === '/api/merchant/profile') return respond(profile)
    if (path === '/api/merchant-pool/routes' && method === 'GET') return respond({ items: routes, capacity: 32 })
    if (path === '/api/merchant-pool/routes' && method === 'POST') {
      const body = request.postDataJSON(); routeRequests.push(body)
      if (routeFailures-- > 0) return respond({ error: { code: 'conflict' } }, 409)
      routes = [{ id: 21, ...body }]; return respond(routes[0], 201)
    }
    if (path === '/api/merchant-pool/routes/preview' && method === 'POST') {
      const body = request.postDataJSON(); previewRequests.push(body)
      assert.deepEqual(body.route.excluded_merchant_ids, [7])
      assert.equal(body.model, 'gpt-fixture')
      assert.equal(body.input_tokens, 1000)
      assert.equal(body.output_tokens, 1000)
      if (previewFailures-- > 0) return respond({ error: { code: 'unavailable' } }, 503)
      return respond({ items: [{ merchant_id: 6, merchant_name: 'CI merchant', service_id: 51, service_name: 'CI service', sales_ratio_micros: 800000, estimated_quota: 1200, eligible: true, reason: null }, { merchant_id: 7, merchant_name: 'CI excluded merchant', service_id: 52, service_name: 'CI excluded service', sales_ratio_micros: 800000, estimated_quota: null, eligible: false, reason: 'excluded' }], quota_per_usd: 500000, as_of: now })
    }
    if (path === '/api/merchant/services' && method === 'GET') return respond(list(services))
    if (path === '/api/merchant/services' && method === 'POST') {
      const body = request.postDataJSON()
      assert.equal(body.api_key, 'ci-upstream-private-key')
      assert.equal(body.models[0].model, 'gpt-fixture')
      const { api_key, pool_id, name, description, ...revision } = body
      assert.ok(api_key)
      services = [{ id: 88, merchant_id: 6, pool_id, name, description, status: 'draft', published: null, draft: { id: 9, revision: 1, status: 'draft', name, description, ...revision, has_api_key: true, review_reason: null, created_at: now, updated_at: now }, created_at: now, updated_at: now }]
      return respond(services[0], 201)
    }
    if (path === '/api/merchant/finance') return respond({ merchant_id: 6, pending_quota: 0, available_quota: 100000, withdrawing_quota: 0, frozen_quota: 0, debt_quota: 0, total_gross_quota: 110000, total_commission_quota: 10000, total_paid_quota: 0, quota_per_usd: 500000 })
    if (path === '/api/merchant/finance/entries') return respond({ items: [], next_cursor: null })
    if (path === '/api/merchant/withdrawals' && method === 'GET') return respond({ items: [], next_cursor: null })
    if (path === '/api/merchant/withdrawals' && method === 'POST') {
      const body = request.postDataJSON(); withdrawalRequests.push(body)
      if (withdrawalFailures-- > 0) return respond({ error: { code: 'unavailable' } }, 503)
      return respond({ id: 5, merchant_id: 6, ...body, status: 'pending', reference: null, reason: null, created_at: now, updated_at: now })
    }
    if (path === '/api/user-routes') return respond({ items: [], capacity: 32 })
    if (path === '/api/token-groups') return respond({ groups: [], next_cursor: null })
    if (path === '/api/models') return respond({ pricing_scope: 'group', next_cursor: null, models: [] })
    if (path === '/api/tokens' && method === 'POST') { const body = request.postDataJSON(); assert.equal(body.user_route_id, 21); assert.equal(body.group_id, null); return respond({ token: { id: 5, name: body.name } }, 201) }
    return respond({ error: { code: 'not_found' } }, 404)
  })
  await page.goto(`${origin}/.merchant-pool-smoke.html`)
  await page.getByRole('heading', { name: label('title'), exact: true }).waitFor()
  await page.getByRole('textbox', { name: label('model'), exact: true }).fill('gpt-fixture')
  await page.getByText('90.0%', { exact: true }).waitFor()
  await page.getByText(label('lowSamples'), { exact: true }).waitFor()
  const catalogTable = page.getByRole('region', { name: label('catalogTable'), exact: true })
  await catalogTable.getByRole('row').filter({ hasText: 'CI service' }).getByText(label('priceInput').replace('{{value}}', '0.80'), { exact: true }).waitFor()
  assert.equal(await catalogTable.locator('tbody tr').count(), 3, 'Each service renders exactly one compact table row')
  assert.equal(await catalogTable.getByRole('columnheader').count(), 10)
  const desktopActions = await catalogTable.getByRole('columnheader', { name: label('tableColumns.actions'), exact: true }).boundingBox()
  assert.ok(desktopActions && desktopActions.x >= 0 && desktopActions.x + desktopActions.width <= 1280, 'Desktop actions remain visible without scrolling the table')
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog.png'), fullPage: true, animations: 'disabled' })
  await page.getByRole('button', { name: 'routes', exact: true }).click()
  await page.getByRole('button', { name: label('createRoute'), exact: true }).click()
  let dialog = page.getByRole('dialog')
  await dialog.getByRole('textbox', { name: label('name'), exact: true }).fill('CI merchant route')
  await dialog.getByRole('button', { name: label('addMerchant'), exact: true }).click()
  await dialog.getByRole('combobox', { name: label('merchant'), exact: true }).selectOption('6')
  await dialog.getByRole('combobox', { name: label('excludedMerchants'), exact: true }).selectOption('7')
  await dialog.getByRole('button', { name: label('addExclusion'), exact: true }).click()
  await dialog.getByRole('textbox', { name: label('model'), exact: true }).fill('gpt-fixture')
  await dialog.getByRole('button', { name: label('previewCandidates'), exact: true }).click()
  await dialog.getByText(label('previewError'), { exact: true }).waitFor()
  assert.equal(await dialog.getByRole('textbox', { name: label('name'), exact: true }).inputValue(), 'CI merchant route')
  await dialog.getByRole('button', { name: label('previewCandidates'), exact: true }).click()
  await dialog.getByText(label('previewEligible'), { exact: true }).waitFor()
  await dialog.getByText(label('previewReasons.excluded'), { exact: true }).waitFor()
  assert.equal(previewRequests.length, 2)
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByRole('alert').waitFor()
  assert.equal(await dialog.getByRole('textbox', { name: label('name'), exact: true }).inputValue(), 'CI merchant route')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(routeRequests.length, 2)
  const savedRoutesTable = page.getByRole('region', { name: label('routes'), exact: true })
  await savedRoutesTable.getByRole('cell', { name: /CI merchant route/ }).waitFor()
  assert.equal(await savedRoutesTable.locator('tbody tr').count(), 1, 'A saved merchant route appears as one table row')
  await page.screenshot({ path: resolve(screenshots, 'merchant-routes.png'), fullPage: true, animations: 'disabled' })
  assert.deepEqual(routeRequests[0].members, [{ merchant_id: 6, priority: 0, weight: 1 }])
  assert.equal(routeRequests[0].accept_new_merchants, false)
  assert.equal(routeRequests[0].allow_pool_fallback, false)
  assert.deepEqual(routeRequests[0].excluded_merchant_ids, [7])
  await page.getByRole('button', { name: 'workspace', exact: true }).click()
  await page.getByRole('button', { name: label('createService'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('textbox', { name: label('name'), exact: true }).fill('CI submitted service')
  await dialog.getByRole('textbox', { name: label('baseUrl'), exact: true }).fill('https://upstream.example.com/v1')
  await dialog.getByLabel(label('upstreamKey'), { exact: true }).fill('ci-upstream-private-key')
  const modelInput = dialog.getByRole('textbox', { name: label('canonicalModel'), exact: true })
  await modelInput.fill('gpt-fixture')
  assert.equal(await modelInput.evaluate(input => input === document.activeElement), true)
  await dialog.getByRole('button', { name: label('saveDraft'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  const ownedServicesTable = page.getByRole('region', { name: label('myServices'), exact: true })
  await ownedServicesTable.getByRole('cell', { name: /CI submitted service/ }).waitFor()
  assert.equal(await ownedServicesTable.locator('tbody tr').count(), 1, 'A created merchant service appears as one table row')
  await page.screenshot({ path: resolve(screenshots, 'merchant-workspace.png'), fullPage: true, animations: 'disabled' })
  assert.equal(await page.evaluate(() => Object.values(sessionStorage).some(value => value.includes('ci-upstream-private-key'))), false)
  await page.getByRole('button', { name: label('finance'), exact: true }).click()
  await page.getByRole('textbox', { name: label('withdrawalAmount'), exact: true }).fill('5000')
  await page.getByRole('button', { name: label('requestWithdrawal'), exact: true }).click()
  await page.getByRole('alert').waitFor()
  await page.getByRole('button', { name: label('requestWithdrawal'), exact: true }).click()
  await page.waitForFunction(() => !document.querySelector('input[aria-label="提现金额（积分）"]')?.value)
  assert.equal(withdrawalRequests.length, 2)
  assert.equal(withdrawalRequests[0].event_key, withdrawalRequests[1].event_key)
  assert.equal(withdrawalRequests[0].amount_quota, 5000)
  await page.getByRole('button', { name: 'key', exact: true }).click()
  await page.getByRole('textbox', { name: commonLabel('apiKeys.fields.name'), exact: true }).fill('CI merchant key')
  await page.getByRole('button', { name: commonLabel('apiKeys.actions.create'), exact: true }).click()
  await page.getByTestId('issued').waitFor()
  await page.setViewportSize({ width: 390, height: 844 })
  await page.getByRole('button', { name: 'catalog', exact: true }).click()
  await page.getByRole('heading', { name: label('title'), exact: true }).waitFor()
  const mobileTable = page.getByRole('region', { name: label('catalogTable'), exact: true })
  const mobileGeometry = await mobileTable.evaluate(node => ({ viewport: innerWidth, pageWidth: document.documentElement.scrollWidth, scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }))
  assert.ok(mobileGeometry.scrollWidth > mobileGeometry.clientWidth, 'Mobile merchant rows scroll within their table')
  assert.ok(mobileGeometry.pageWidth <= mobileGeometry.viewport, 'The table must not force the mobile page beyond the viewport')
  assert.equal(await mobileTable.locator('tbody tr').count(), 3)
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-mobile.png'), fullPage: true, animations: 'disabled' })
  await mobileTable.evaluate(node => { node.scrollLeft = node.scrollWidth })
  const fixedMerchant = await mobileTable.locator('tbody tr').first().locator('td').first().boundingBox()
  assert.ok(fixedMerchant && fixedMerchant.x >= 0 && fixedMerchant.x < mobileGeometry.viewport, 'Merchant identity remains visible while scrolling')
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-mobile-actions.png'), fullPage: true, animations: 'disabled' })
  assert.deepEqual(errors, [])
  console.log('Merchant pool browser regression passed')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ errors, routeRequests, withdrawalRequests, ariaSnapshot: page ? await page.locator('body').ariaSnapshot().catch(() => undefined) : undefined }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
