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
const offer = { model: 'gpt-fixture', sales_ratio_micros: 800000, context_window: 128000, max_output_tokens: 4096, supports_streaming: true, supports_tools: true, supports_vision: false, prices: { currency: 'USD', unit: 'million_tokens', billing_mode: 'token', video_per_second: null, input: '0.80', output: '1.60', cache_read: '0.08', cache_write_5m: '1.00', cache_write_1h: '1.60', version: 1 } }
const publicService = { id: 51, pool_id: 1, merchant_id: 6, merchant_name: 'CI merchant', name: 'CI service', description: 'Reviewed service', status: 'published', revision: 1, price_version: 0, protocol: 'openai_chat', maintenance: [], models: [offer], updated_at: now }
const excludedService = { ...publicService, id: 52, merchant_id: 7, merchant_name: 'CI excluded merchant', name: 'CI excluded service', description: '面向长文本和日常开发的独立渠道，商家公开说明会保留完整内容，但在市场行内收起以保持模型、报价和运行表现易于比较。'.repeat(3) }
const reserveService = { ...publicService, id: 53, merchant_id: 8, merchant_name: 'CI reserve merchant', name: 'CI reserve service', description: '稳定优先，适合开发与备用切换。' }
const timelineBuckets = Array.from({ length: 24 }, (_, index) => ({ started_at: now - (24 - index) * 3600, ended_at: now - (23 - index) * 3600, success_count: index >= 14 && index !== 20 ? 1 : 0, failure_count: index === 20 ? 1 : 0, sample_count: index >= 14 ? 1 : 0, status: index < 14 ? 'unknown' : index === 20 ? 'degraded' : 'normal' }))
const profile = { id: 6, owner_user_id: 3, name: 'CI merchant', description: 'CI relay', status: 'approved', review_reason: null, created_at: now, updated_at: now }
let activeProfile = null
const list = items => ({ items, total: items.length, page: 1, page_size: 20 })
let routes = []
let services = []
let adminPools = [pool]
let adminMerchants = [profile]
let poolDeleteFailures = 1, merchantDeleteFailures = 1
const poolDeleteRequests = [], merchantDeleteRequests = []
let poolFailures = 1
let routeFailures = 1
let previewFailures = 1
let withdrawalFailures = 1
let priceFailures = 1, noticeFailures = 1, accountFailures = 1, payoutSettingsFailures = 1
let prices = [], notices = [], withdrawals = [], payouts = []
let payoutAccount = { configured: false, label: '', version: 0 }
let payoutSettings = { version: 0, enabled: false, auto_approve_enabled: false, gateway_url: '', currency: 'USD', quota_per_unit: 500000, min_amount_minor: 100, max_amount_minor: 100000, daily_limit_minor: 1000000, signing_key_configured: false }
const priceRequests = [], noticeRequests = [], accountRequests = [], payoutSettingsRequests = []
let payoutSubmissions = 0, payoutQueries = 0
const videoService = { ...publicService, id: 61, protocol: 'xai_video', name: 'CI video service', models: [{ ...offer, model: 'grok-imagine-video', supports_streaming: false, supports_tools: false, prices: { ...offer.prices, unit: 'second', billing_mode: 'video', video_per_second: { p480: '0.04', p720: '0.06', p1080: null } } }] }
const routeRequests = []
const previewRequests = []
const withdrawalRequests = []
const catalogRequests = []
const poolRequests = []
const serviceRequests = []
const quoteRequests = []
const applicationRequests = []
const errors = []
let server, browser, page
let serverOutput = ''

const documentGeometry = () => {
  const describe = node => {
    const style = getComputedStyle(node)
    const rect = node.getBoundingClientRect()
    return { tag: node.tagName, className: node.getAttribute('class'), slot: node.getAttribute('data-slot'), inlineStyle: node.getAttribute('style'), left: rect.left, right: rect.right, rectWidth: rect.width, clientWidth: node.clientWidth, scrollWidth: node.scrollWidth, width: style.width, minWidth: style.minWidth, maxWidth: style.maxWidth, paddingLeft: style.paddingLeft, paddingRight: style.paddingRight, marginLeft: style.marginLeft, marginRight: style.marginRight, boxSizing: style.boxSizing, position: style.position, overflowX: style.overflowX, overflowY: style.overflowY, scrollbarGutter: style.scrollbarGutter }
  }
  const visibleOverflow = [...document.querySelectorAll('body *')].filter(node => {
    const style = getComputedStyle(node)
    const rect = node.getBoundingClientRect()
    if (style.display === 'none' || style.visibility !== 'visible' || Number(style.opacity) === 0 || rect.width === 0 || (rect.left >= -1 && rect.right <= innerWidth + 1)) return false
    for (let ancestor = node.parentElement; ancestor && ancestor !== document.body; ancestor = ancestor.parentElement) {
      if (['auto', 'scroll', 'hidden', 'clip'].includes(getComputedStyle(ancestor).overflowX)) return false
    }
    return true
  }).slice(0, 30).map(describe)
  return { viewport: innerWidth, viewportHeight: innerHeight, scrollX, html: describe(document.documentElement), body: describe(document.body), scrollLocked: document.body.getAttribute('data-scroll-locked'), scrollbarWidth: getComputedStyle(document.body).getPropertyValue('--scrollbar-width'), activeDialogs: [...document.querySelectorAll('[role="dialog"]')].map(describe), visibleOverflow }
}

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
    import { useSiteTheme } from './src/shared/components/use-site-theme';
    import { MerchantPoolPage } from './src/features/merchant-pool/merchant-pool-page';
    import { MerchantPoolRoutesPage } from './src/features/merchant-pool/merchant-pool-routes-page';
    import { MerchantAdminPage } from './src/features/merchant-pool/merchant-admin-page';
    import { MerchantWorkspacePage } from './src/features/merchant-pool/merchant-workspace-page';
    import { ApiKeyForm } from './src/features/api-keys/api-key-form';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function App() { useSiteTheme(); const [view, setView] = useState('catalog'); const [issued, setIssued] = useState(null); return <HeroUIProvider><QueryClientProvider client={client}><main style={{ padding: 16, maxWidth: 1200, margin: 'auto' }}><nav aria-label="CI views">{['catalog','routes','workspace','admin','key'].map(value => <button key={value} onClick={() => setView(value)}>{value}</button>)}</nav>{view === 'catalog' ? <MerchantPoolPage /> : view === 'routes' ? <MerchantPoolRoutesPage /> : view === 'workspace' ? <MerchantWorkspacePage /> : view === 'admin' ? <MerchantAdminPage /> : <ApiKeyForm initialRouteId={21} onCancel={() => setView('routes')} onSaved={() => setView('routes')} onIssued={setIssued} />}{issued ? <output data-testid="issued">issued</output> : null}</main></QueryClientProvider></HeroUIProvider> }
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
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); localStorage.setItem('anyflows.theme', 'dark'); sessionStorage.setItem('anyflows.management.access-token', 'ci-merchant') })
  await page.route(url => url.pathname.startsWith('/api/'), async intercepted => {
    const request = intercepted.request(), path = new URL(request.url()).pathname, method = request.method()
    const respond = (body, status = 200) => intercepted.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    if (path === '/api/site') return respond({ balance_display: { mode: 'quota', unit_name: '积分', unit_symbol: '积分', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } })
    assert.equal(request.headers().authorization, 'Bearer ci-merchant')
    if ((path === '/api/merchant-pools' || path === '/api/merchant/available-pools') && method === 'GET') return respond(list([pool]))
    if (path === '/api/admin/groups' && method === 'GET') return respond({ groups: [{ id: 8, name: 'CI occupied group' }, { id: 9, name: 'CI dedicated group' }, { id: 10, name: 'CI second dedicated group' }], next_cursor: null })
    if (path === '/api/admin/merchant-pools' && method === 'GET') return respond(list(adminPools))
    if (/^\/api\/admin\/merchant-pools\/\d+$/.test(path) && method === 'DELETE') {
      const id = Number(path.split('/').at(-1)); poolDeleteRequests.push(id)
      if (poolDeleteFailures-- > 0) return respond({ error: { code: 'merchant_pool_in_use' } }, 409)
      adminPools = adminPools.filter(item => item.id !== id)
      return intercepted.fulfill({ status: 204 })
    }
    if (/^\/api\/admin\/merchants\/\d+$/.test(path) && method === 'DELETE') {
      const id = Number(path.split('/').at(-1)); merchantDeleteRequests.push(id)
      if (merchantDeleteFailures-- > 0) return respond({ error: { code: 'merchant_pool_in_use' } }, 409)
      adminMerchants = adminMerchants.filter(item => item.id !== id)
      services = services.filter(item => item.merchant_id !== id)
      return intercepted.fulfill({ status: 204 })
    }
    if (path === '/api/admin/merchant-pools' && method === 'POST') {
      const body = request.postDataJSON(); poolRequests.push(body)
      assert.ok([9, 10].includes(body.group_id), 'A new pool submits a selected dedicated group')
      if (poolFailures-- > 0) return respond({ error: { code: 'merchant_pool_conflict', message: 'merchant pool state conflict' } }, 409)
      const createdPool = { id: adminPools.length + 1, ...body, effective_commission_bps: body.commission_inherit_global ? 500 : body.commission_bps, created_at: now, updated_at: now }
      adminPools = [...adminPools, createdPool]
      return respond(createdPool, 201)
    }
    if (path === '/api/merchant-pools/1/services') {
      const query = new URL(request.url()).searchParams
      catalogRequests.push(Object.fromEntries(query))
      if (query.get('protocol') === 'xai_video') { assert.equal(query.has('streaming'), false); return respond(list([videoService])) }
      return respond(list(query.get('model') ? [publicService, excludedService, reserveService] : [publicService, excludedService]))
    }
    if (path === '/api/merchant-pools/1/statistics') return respond({ items: [{ service_id: 51, merchant_id: 6, sample_count: 10, success_count: 9, failure_count: 1, success_rate: 90, ttft_p50_ms: 123, ttft_p95_ms: 400, latency_p50_ms: 300, latency_p95_ms: 900, status: 'normal', updated_at: now }], as_of: now, window_seconds: 86400 })
    if (path === '/api/merchant-pools/1/timeline') return respond({ items: [{ service_id: 51, merchant_id: 6, buckets: timelineBuckets }], as_of: now, window_seconds: 86400 })
    if (path === '/api/merchant/profile' && method === 'GET') return respond(activeProfile)
    if (path === '/api/merchant/apply' && method === 'POST') {
      const body = request.postDataJSON(); applicationRequests.push(body)
      activeProfile = { ...profile, name: body.name, description: body.description, status: 'pending' }
      return respond(activeProfile, 201)
    }
    if (path === '/api/merchant-pool/routes' && method === 'GET') return respond({ items: routes, capacity: 32 })
    if (path === '/api/merchant-pool/routes' && method === 'POST') {
      const body = request.postDataJSON(); routeRequests.push(body)
      if (routeFailures-- > 0) return respond({ error: { code: 'conflict' } }, 409)
      routes = [{ id: 21, ...body }]; return respond(routes[0], 201)
    }
    if (path === '/api/merchant-pool/routes/preview' && method === 'POST') {
      const body = request.postDataJSON(); previewRequests.push(body)
      assert.deepEqual(body.route.excluded_merchant_ids, [7])
      assert.equal(body.model, body.protocol === 'xai_video' ? 'grok-imagine-video' : 'gpt-fixture')
      assert.equal(body.input_tokens, 1000)
      assert.equal(body.output_tokens, 1000)
      if (previewFailures-- > 0) return respond({ error: { code: 'unavailable' } }, 503)
      return respond({ items: [{ merchant_id: 6, merchant_name: 'CI merchant', service_id: 51, service_name: 'CI service', sales_ratio_micros: 800000, estimated_quota: 1200, eligible: true, reason: null }, { merchant_id: 7, merchant_name: 'CI excluded merchant', service_id: 52, service_name: 'CI excluded service', sales_ratio_micros: 800000, estimated_quota: null, eligible: false, reason: 'excluded' }], quota_per_usd: 500000, as_of: now })
    }
    if ((path === '/api/merchant/services' || path === '/api/admin/merchant-services') && method === 'GET') return respond(list(services))
    if (path === '/api/merchant/services' && method === 'POST') {
      const body = request.postDataJSON()
      serviceRequests.push(body)
      assert.equal(body.api_key, 'ci-upstream-private-key')
      assert.equal(body.models[0].model, 'gpt-fixture')
      const { api_key, pool_id, name, description, ...revision } = body
      assert.ok(api_key)
      services = [{ id: 88, merchant_id: 6, pool_id, name, description, status: 'draft', active_price: null, maintenance: [], published: null, draft: { id: 9, revision: 1, status: 'draft', name, description, ...revision, has_api_key: true, review_reason: null, created_at: now, updated_at: now }, created_at: now, updated_at: now }]
      return respond(services[0], 201)
    }
    if (path === '/api/merchant/quote' && method === 'POST') {
      const body = request.postDataJSON(); quoteRequests.push(body)
      return respond({ models: [{ model: body.models[0].model, sales_ratio_micros: body.default_ratio_micros, base_prices: offer.prices, prices: offer.prices, net_prices: offer.prices }], effective_commission_bps: 500 })
    }
    if (path === '/api/merchant/finance') return respond({ merchant_id: 6, pending_quota: 0, available_quota: 100000, withdrawing_quota: 0, frozen_quota: 0, debt_quota: 0, total_gross_quota: 110000, total_commission_quota: 10000, total_paid_quota: 0, quota_per_usd: 500000 })
    if (path === '/api/merchant/finance/entries') return respond({ items: [], next_cursor: null })
    if ((path === '/api/merchant/withdrawals' || path === '/api/admin/merchant-withdrawals') && method === 'GET') return respond({ items: withdrawals, next_cursor: null })
    if (path === '/api/merchant/withdrawals' && method === 'POST') {
      const body = request.postDataJSON(); withdrawalRequests.push(body)
      if (withdrawalFailures-- > 0) return respond({ error: { code: 'unavailable' } }, 503)
      withdrawals = [{ id: 5, merchant_id: 6, ...body, payout: null, status: 'pending', reference: null, reason: null, created_at: now, updated_at: now }]; return respond(withdrawals[0])
    }
    if (path === '/api/admin/merchants') return respond(list(adminMerchants))
    if (path === '/api/admin/merchant-pool-settings') return respond({ commission_bps: 500 })
    if (path === '/api/merchant/services/88/prices' && method === 'GET') return respond(list(prices))
    if (path === '/api/merchant/services/88/prices' && method === 'POST') {
      const body = request.postDataJSON(); priceRequests.push(body)
      assert.equal(body.expected_revision_id, 9)
      assert.equal(body.default_ratio_micros, 900000)
      assert.ok(body.effective_at > now)
      if (priceFailures-- > 0) return respond({ error: { code: 'conflict' } }, 409)
      prices = [{ id: 71, service_id: 88, revision_id: 9, ...body, status: 'scheduled', reason: null, created_at: now, updated_at: now }]
      return respond(prices[0], 201)
    }
    if (path === '/api/merchant/services/88/prices/71/cancel') { prices[0].status = 'cancelled'; return respond(prices[0]) }
    if (path === '/api/merchant/services/88/maintenance' && method === 'GET') return respond(list(notices))
    if (path === '/api/merchant/services/88/maintenance' && method === 'POST') {
      const body = request.postDataJSON(); noticeRequests.push(body)
      if (noticeFailures-- > 0) return respond({ error: { code: 'conflict' } }, 409)
      notices = [{ id: 81, service_id: 88, ...body, status: 'active', created_at: now, updated_at: now }]
      return respond(notices[0], 201)
    }
    if (path === '/api/merchant/payout-account' && method === 'GET') return respond(payoutAccount)
    if (path === '/api/merchant/payout-account' && method === 'PUT') {
      const body = request.postDataJSON(); accountRequests.push(body)
      if (accountFailures-- > 0) return respond({ error: { code: 'unavailable' } }, 503)
      assert.equal(body.expected_version, payoutAccount.version)
      payoutAccount = { configured: !body.clear, label: body.label, version: payoutAccount.version + 1 }; return respond(payoutAccount)
    }
    if ((path === '/api/merchant/payouts' || path === '/api/admin/merchant-payouts') && method === 'GET') return respond({ items: payouts, next_cursor: null })
    if (path === '/api/admin/merchant-payout-settings' && method === 'GET') return respond(payoutSettings)
    if (path === '/api/admin/merchant-payout-settings' && method === 'PUT') {
      const body = request.postDataJSON(); payoutSettingsRequests.push(body)
      if (payoutSettingsFailures-- > 0) return respond({ error: { code: 'conflict' } }, 409)
      assert.equal(body.expected_version, payoutSettings.version)
      const { expected_version, signing_key, clear_signing_key, ...fields } = body
      payoutSettings = { ...fields, version: expected_version + 1, signing_key_configured: !clear_signing_key && (Boolean(signing_key) || payoutSettings.signing_key_configured) }; return respond(payoutSettings)
    }
    if (path === '/api/admin/merchant-withdrawals/5/payout') {
      payoutSubmissions += 1; assert.equal(payoutSubmissions, 1)
      payouts = [{ withdrawal_id: 5, merchant_id: 6, event_key: withdrawals[0].event_key, status: 'unknown', amount_minor: 1, currency: 'USD', reference: null, reason: 'gateway_timeout', created_at: now, updated_at: now }]
      withdrawals[0].payout = payouts[0]; return respond(payouts[0])
    }
    if (path === '/api/admin/merchant-withdrawals/5/reconcile') { payoutQueries += 1; return respond(payouts[0]) }
    if (path === '/api/user-routes') return respond({ items: [], capacity: 32 })
    if (path === '/api/token-groups') return respond({ groups: [], next_cursor: null })
    if (path === '/api/models') return respond({ pricing_scope: 'group', next_cursor: null, models: [] })
    if (path === '/api/tokens' && method === 'POST') { const body = request.postDataJSON(); assert.equal(body.user_route_id, 21); assert.equal(body.group_id, null); return respond({ token: { id: 5, name: body.name } }, 201) }
    return respond({ error: { code: 'not_found' } }, 404)
  })
  await page.goto(`${origin}/.merchant-pool-smoke.html`)
  await page.getByRole('heading', { name: label('market.title'), exact: true }).waitFor()
  await page.getByRole('textbox', { name: label('model'), exact: true }).fill('gpt-fixture')
  await page.getByText('90.0%', { exact: true }).waitFor()
  await page.getByText(label('lowSamples'), { exact: true }).waitFor()
  const catalogTable = page.getByRole('region', { name: label('catalogTable'), exact: true })
  const catalogRow = catalogTable.getByRole('row').filter({ hasText: 'CI service' })
  await catalogRow.getByText(label('priceInput').replace('{{value}}', '0.80'), { exact: true }).waitFor()
  assert.equal(await catalogRow.getByText(publicService.description, { exact: true }).isVisible(), true, 'Reviewed merchant introductions are visible without hovering')
  assert.equal(await catalogTable.getByRole('row').filter({ hasText: excludedService.name }).getByText(excludedService.description, { exact: true }).isVisible(), true, 'Long introductions stay present while the row clamps their display')
  assert.equal(await catalogTable.locator('tbody tr').count(), 3, 'Each service renders exactly one compact table row')
  assert.equal(await catalogTable.getByRole('columnheader').count(), 7)
  const desktopGeometry = await catalogTable.evaluate(node => ({ viewport: innerWidth, pageWidth: document.documentElement.scrollWidth, left: node.getBoundingClientRect().left }))
  assert.ok(desktopGeometry.pageWidth <= desktopGeometry.viewport, 'The market filters and long descriptions remain inside the desktop viewport')
  const desktopActions = await catalogTable.getByRole('columnheader', { name: label('tableColumns.actions'), exact: true }).boundingBox()
  assert.ok(desktopActions && desktopActions.x >= 0 && desktopActions.x + desktopActions.width <= 1280, 'Desktop actions remain visible without scrolling the table')
  await catalogTable.evaluate(node => { node.scrollLeft = node.scrollWidth })
  const desktopSource = await catalogTable.locator('tbody tr').first().locator('td').first().boundingBox()
  assert.ok(desktopSource && Math.abs(desktopSource.x - desktopGeometry.left) < 2, 'The model and merchant identity remain fixed while the desktop table scrolls')
  await catalogTable.evaluate(node => { node.scrollLeft = 0 })
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('dark'); document.documentElement.classList.add('light') })
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-light.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('light'); document.documentElement.classList.add('dark') })
  await page.getByRole('button', { name: 'admin', exact: true }).click()
  await page.getByRole('button', { name: label('createPool'), exact: true }).click()
  let dialog = page.getByRole('dialog')
  const poolName = dialog.getByRole('textbox', { name: label('name'), exact: true })
  const poolGroup = dialog.getByRole('combobox', { name: label('group'), exact: true })
  await poolGroup.getByRole('option', { name: 'CI dedicated group', exact: true }).waitFor({ state: 'attached' })
  assert.equal(await poolGroup.inputValue(), '', 'Creating a pool never silently chooses a group')
  assert.equal(await poolGroup.getByRole('option', { name: label('validation.selectGroup'), exact: true }).count(), 1, 'The required group placeholder does not say all groups')
  await poolName.fill('CI inherited pool')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('validation.groupRequired'), { exact: true }).first().waitFor()
  assert.equal(poolRequests.length, 0, 'A missing required group is rejected before submitting')
  await poolGroup.selectOption('9')
  await dialog.getByText(label('validation.groupRequired'), { exact: true }).first().waitFor({ state: 'hidden' })
  const poolDescription = dialog.getByRole('textbox', { name: label('description'), exact: true })
  const multilinePoolDescription = 'First pool line\nSecond pool line\r\nThird pool line'
  await poolDescription.fill('Unsupported\ttab')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('validation.singleLine'), { exact: true }).first().waitFor()
  assert.equal(poolRequests.length, 0, 'Tabs remain invalid control characters in pool descriptions')
  await poolDescription.fill(multilinePoolDescription)
  await dialog.getByText(label('validation.singleLine'), { exact: true }).first().waitFor({ state: 'hidden' })
  await dialog.getByRole('checkbox', { name: label('inheritGlobalCommission'), exact: true }).uncheck()
  const poolFee = dialog.getByRole('textbox', { name: label('commissionRate'), exact: true })
  await poolFee.fill('12.345')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('validation.commission'), { exact: true }).first().waitFor()
  assert.equal(poolRequests.length, 0)
  await dialog.getByRole('checkbox', { name: label('inheritGlobalCommission'), exact: true }).check()
  assert.equal(await poolFee.isDisabled(), true)
  await dialog.getByText(label('validation.commission'), { exact: true }).first().waitFor({ state: 'hidden' })
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('validation.groupConflict'), { exact: true }).first().waitFor()
  assert.equal(await poolName.inputValue(), 'CI inherited pool', 'A server-side group conflict preserves the draft')
  assert.equal(await poolGroup.inputValue(), '9')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(poolRequests.length, 2, 'An invalid disabled fee cannot block an inherited commission')
  assert.deepEqual(poolRequests[0], poolRequests[1])
  assert.deepEqual(poolRequests[1], { name: 'CI inherited pool', description: multilinePoolDescription, group_id: 9, enabled: true, commission_bps: 0, commission_inherit_global: true, min_ratio_micros: 100000, max_ratio_micros: 10000000, settlement_delay_secs: 86400 })
  await page.getByRole('region', { name: label('adminTabs.pools'), exact: true }).getByText('CI inherited pool', { exact: true }).waitFor()
  await page.getByRole('button', { name: label('createPool'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('textbox', { name: label('name'), exact: true }).fill('CI explicit pool')
  await dialog.getByRole('combobox', { name: label('group'), exact: true }).selectOption('10')
  const minimumPoolRatio = dialog.getByRole('textbox', { name: label('minimumRatio'), exact: true })
  const maximumPoolRatio = dialog.getByRole('textbox', { name: label('maximumRatio'), exact: true })
  const poolDelay = dialog.getByRole('spinbutton', { name: label('settlementDelay'), exact: true })
  await minimumPoolRatio.fill('0')
  await maximumPoolRatio.fill('101')
  await poolDelay.fill('2161')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('validation.settlementDelay'), { exact: true }).first().waitFor()
  assert.equal(await minimumPoolRatio.getAttribute('aria-invalid'), 'true')
  assert.equal(await maximumPoolRatio.getAttribute('aria-invalid'), 'true')
  assert.equal(await poolDelay.getAttribute('aria-invalid'), 'true')
  assert.equal(poolRequests.length, 2, 'Pool boundary errors never reach the server')
  await minimumPoolRatio.fill('0.1')
  await maximumPoolRatio.fill('10')
  await poolDelay.fill('24')
  await dialog.getByRole('checkbox', { name: label('inheritGlobalCommission'), exact: true }).uncheck()
  await dialog.getByRole('textbox', { name: label('commissionRate'), exact: true }).fill('15.25')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(poolRequests.length, 3)
  assert.equal(poolRequests[2].commission_bps, 1525)
  assert.equal(poolRequests[2].commission_inherit_global, false)
  assert.equal(poolRequests[2].group_id, 10)
  await page.screenshot({ path: resolve(screenshots, 'merchant-admin-pools.png'), fullPage: true, animations: 'disabled' })
  await page.getByRole('button', { name: 'routes', exact: true }).click()
  await page.getByRole('button', { name: label('createRoute'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('button', { name: label('save'), exact: true }).click()
  await dialog.getByText(label('invalidForm'), { exact: true }).first().waitFor()
  assert.equal(routeRequests.length, 0)
  await dialog.getByRole('textbox', { name: label('name'), exact: true }).fill('CI merchant route')
  await dialog.getByText(label('invalidForm'), { exact: true }).first().waitFor({ state: 'hidden' })
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
  await dialog.getByRole('combobox', { name: label('protocol'), exact: true }).selectOption('xai_video')
  await dialog.getByRole('textbox', { name: label('model'), exact: true }).fill('grok-imagine-video')
  await dialog.getByRole('spinbutton', { name: label('videoDuration'), exact: true }).fill('12')
  await dialog.getByRole('combobox', { name: label('videoResolution'), exact: true }).selectOption('720p')
  const videoPreviewResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/api/merchant-pool/routes/preview' && response.request().postDataJSON().protocol === 'xai_video')
  await dialog.getByRole('button', { name: label('previewCandidates'), exact: true }).click()
  await videoPreviewResponse
  assert.equal(previewRequests.at(-1).duration_seconds, 12)
  assert.equal(previewRequests.at(-1).resolution, '720p')
  assert.equal(previewRequests.at(-1).streaming, false)
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
  const applicationName = page.getByRole('textbox', { name: label('merchantName'), exact: true })
  const applicationDescription = page.getByRole('textbox', { name: label('description'), exact: true })
  await applicationName.fill('商'.repeat(43))
  await page.getByRole('button', { name: label('submitApplication'), exact: true }).click()
  await page.getByText(label('validation.textBytes').replace('{{max}}', '128'), { exact: true }).first().waitFor()
  assert.equal(applicationRequests.length, 0, 'UTF-8 byte limits are checked before applying as a merchant')
  await applicationName.fill('CI merchant application')
  await page.getByText(label('validation.textBytes').replace('{{max}}', '128'), { exact: true }).first().waitFor({ state: 'hidden' })
  await applicationDescription.fill('CI merchant\tintroduction')
  await page.getByRole('button', { name: label('submitApplication'), exact: true }).click()
  await page.getByText(label('validation.singleLine'), { exact: true }).first().waitFor()
  assert.equal(applicationRequests.length, 0)
  const multilineApplicationDescription = 'CI merchant introduction\nSecond line\r\nThird line'
  await applicationDescription.fill(multilineApplicationDescription)
  await page.getByText(label('validation.singleLine'), { exact: true }).first().waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('submitApplication'), exact: true }).click()
  await page.getByText(label('awaitApproval'), { exact: true }).waitFor()
  assert.deepEqual(applicationRequests, [{ name: 'CI merchant application', description: multilineApplicationDescription }], 'Merchant applications retain supported multiline descriptions')
  activeProfile = profile
  await page.getByRole('button', { name: label('refresh'), exact: true }).click()
  await page.getByRole('button', { name: label('createService'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('textbox', { name: label('name'), exact: true }).fill('CI submitted service')
  await dialog.getByRole('textbox', { name: label('baseUrl'), exact: true }).fill('https://upstream.example.com/v1')
  await dialog.getByLabel(label('upstreamKey'), { exact: true }).fill('ci-upstream-private-key')
  const modelInput = dialog.getByRole('textbox', { name: label('canonicalModel'), exact: true })
  await modelInput.fill('gpt-fixture')
  assert.equal(await modelInput.evaluate(input => input === document.activeElement), true)
  const serviceConcurrency = dialog.getByRole('spinbutton', { name: label('concurrency'), exact: true })
  const serviceUrl = dialog.getByRole('textbox', { name: label('baseUrl'), exact: true })
  const serviceContext = dialog.getByRole('spinbutton', { name: label('contextWindow'), exact: true })
  const serviceOutput = dialog.getByRole('spinbutton', { name: label('maxOutput'), exact: true })
  await serviceConcurrency.fill('10001')
  await dialog.getByRole('button', { name: label('saveDraft'), exact: true }).click()
  await dialog.getByText(label('validation.positiveIntegerRange').replace('{{max}}', '10000'), { exact: true }).first().waitFor()
  assert.equal(await serviceConcurrency.getAttribute('aria-invalid'), 'true')
  assert.equal(serviceRequests.length, 0, 'Concurrency outside the server limit is not submitted')
  await serviceConcurrency.fill('10')
  await serviceUrl.fill('https://upstream.example.com:9443/v1')
  await dialog.getByRole('button', { name: label('saveDraft'), exact: true }).click()
  await dialog.getByText(label('validation.httpsEndpoint'), { exact: true }).first().waitFor()
  assert.equal(serviceRequests.length, 0, 'Merchant upstream URLs must use HTTPS on port 443')
  await serviceUrl.fill('https://upstream.example.com/v1')
  await serviceContext.fill('4294967296')
  await dialog.getByRole('button', { name: label('previewPrices'), exact: true }).click()
  await dialog.getByText(label('validation.positiveIntegerRange').replace('{{max}}', '4294967295'), { exact: true }).first().waitFor()
  assert.equal(await serviceContext.getAttribute('aria-invalid'), 'true')
  assert.equal(quoteRequests.length, 0, 'A quote must not silently convert an out-of-range model limit to null')
  await serviceContext.fill('4096')
  await serviceOutput.fill('4097')
  await dialog.getByRole('button', { name: label('saveDraft'), exact: true }).click()
  await dialog.getByText(label('validation.outputExceedsContext'), { exact: true }).first().waitFor()
  assert.equal(serviceRequests.length, 0, 'The output limit cannot exceed the model context window')
  await serviceOutput.fill('4096')
  await dialog.getByText(label('validation.outputExceedsContext'), { exact: true }).first().waitFor({ state: 'hidden' })
  await dialog.getByRole('button', { name: label('previewPrices'), exact: true }).click()
  await dialog.getByRole('heading', { name: label('quotePreview'), exact: true }).waitFor()
  assert.equal(quoteRequests.length, 1)
  assert.equal(quoteRequests[0].models[0].context_window, 4096)
  assert.equal(quoteRequests[0].models[0].max_output_tokens, 4096)
  await dialog.getByRole('button', { name: label('saveDraft'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  assert.equal(serviceRequests.length, 1)
  assert.equal(serviceRequests[0].concurrency, 10)
  assert.equal(serviceRequests[0].models[0].context_window, 4096)
  assert.equal(serviceRequests[0].models[0].max_output_tokens, 4096)
  const ownedServicesTable = page.getByRole('region', { name: label('myServices'), exact: true })
  await ownedServicesTable.getByRole('cell', { name: /CI submitted service/ }).waitFor()
  assert.equal(await ownedServicesTable.locator('tbody tr').count(), 1, 'A created merchant service appears as one table row')
  await page.screenshot({ path: resolve(screenshots, 'merchant-workspace.png'), fullPage: true, animations: 'disabled' })
  assert.equal(await page.evaluate(() => Object.values(sessionStorage).some(value => value.includes('ci-upstream-private-key'))), false)
  services[0] = { ...services[0], status: 'published', published: { ...services[0].draft, status: 'published' }, draft: null }
  await page.getByRole('button', { name: 'routes', exact: true }).click()
  await page.getByRole('button', { name: 'workspace', exact: true }).click()
  await page.getByRole('button', { name: label('serviceLifecycle'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('textbox', { name: label('defaultRatio'), exact: true }).fill('0.9')
  await dialog.getByRole('checkbox', { name: label('effectiveNow'), exact: true }).uncheck()
  await dialog.getByRole('button', { name: label('publishPrice'), exact: true }).click()
  await dialog.getByText(label('saveError'), { exact: true }).waitFor()
  assert.equal(await dialog.getByRole('textbox', { name: label('defaultRatio'), exact: true }).inputValue(), '0.9')
  await dialog.getByRole('button', { name: label('publishPrice'), exact: true }).click()
  await dialog.getByRole('button', { name: label('cancelSchedule'), exact: true }).waitFor()
  assert.equal(priceRequests.length, 2)
  await dialog.getByRole('button', { name: label('cancelSchedule'), exact: true }).click()
  await dialog.getByText(label('status.cancelled'), { exact: true }).waitFor()
  await dialog.getByRole('button', { name: label('lifecycleTabs.maintenance'), exact: true }).click()
  await dialog.getByRole('textbox', { name: label('maintenanceMessage'), exact: true }).fill('CI scheduled maintenance')
  await dialog.getByRole('checkbox', { name: label('maintenanceBlocking'), exact: true }).check()
  await dialog.getByRole('button', { name: label('publishNotice'), exact: true }).click()
  await dialog.getByText(label('saveError'), { exact: true }).waitFor()
  assert.equal(await dialog.getByRole('textbox', { name: label('maintenanceMessage'), exact: true }).inputValue(), 'CI scheduled maintenance')
  await dialog.getByRole('button', { name: label('publishNotice'), exact: true }).click()
  await dialog.getByRole('cell', { name: 'CI scheduled maintenance', exact: true }).waitFor()
  assert.equal(noticeRequests.length, 2)
  assert.equal(noticeRequests.at(-1).blocks_requests, true)
  await page.screenshot({ path: resolve(screenshots, 'merchant-maintenance.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('dark'); document.documentElement.classList.add('light') })
  await page.screenshot({ path: resolve(screenshots, 'merchant-maintenance-light.png'), fullPage: true, animations: 'disabled' })
  await page.setViewportSize({ width: 390, height: 844 })
  await page.screenshot({ path: resolve(screenshots, 'merchant-maintenance-mobile-light.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('light'); document.documentElement.classList.add('dark') })
  await page.screenshot({ path: resolve(screenshots, 'merchant-maintenance-mobile.png'), fullPage: true, animations: 'disabled' })
  await page.setViewportSize({ width: 1280, height: 1000 })
  await dialog.getByRole('button', { name: label('close'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('finance'), exact: true }).click()
  const payoutAccountLabel = page.getByRole('textbox', { name: label('payoutAccountLabel'), exact: true })
  const payoutAccountToken = page.getByLabel(label('payoutAccountToken'), { exact: true })
  await payoutAccountLabel.fill('商'.repeat(27))
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.getByText(label('validation.payoutLabelBytes').replace('{{max}}', '80'), { exact: true }).waitFor()
  assert.equal(accountRequests.length, 0, 'Oversized payout account labels are rejected before PUT')
  await payoutAccountLabel.fill('CI settlement account')
  await page.getByText(label('validation.payoutLabelBytes').replace('{{max}}', '80'), { exact: true }).waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.getByText(label('validation.payoutAccountRequired'), { exact: true }).waitFor()
  assert.equal(accountRequests.length, 0, 'A missing receiving account token is rejected before PUT')
  await payoutAccountToken.fill('ci-private-receiving-token')
  await page.getByText(label('validation.payoutAccountRequired'), { exact: true }).waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.getByText(label('saveError'), { exact: true }).waitFor()
  assert.equal(await page.getByLabel(label('payoutAccountToken'), { exact: true }).inputValue(), 'ci-private-receiving-token')
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.waitForFunction(() => !document.querySelector('input[type="password"]')?.value)
  assert.equal(accountRequests.length, 2)
  assert.equal(accountRequests[0].account, accountRequests[1].account)
  await page.getByRole('textbox', { name: label('withdrawalAmount'), exact: true }).fill('5000')
  await page.getByRole('button', { name: label('requestWithdrawal'), exact: true }).click()
  await page.getByRole('alert').waitFor()
  await page.getByRole('button', { name: label('requestWithdrawal'), exact: true }).click()
  await page.waitForFunction(() => !document.querySelector('input[aria-label="提现金额（积分）"]')?.value)
  assert.equal(withdrawalRequests.length, 2)
  assert.equal(withdrawalRequests[0].event_key, withdrawalRequests[1].event_key)
  assert.equal(withdrawalRequests[0].amount_quota, 5000)
  await page.getByRole('button', { name: 'admin', exact: true }).click()
  await page.getByRole('button', { name: label('adminTabs.payoutSettings'), exact: true }).click()
  await page.getByRole('textbox', { name: label('payoutGateway'), exact: true }).fill('https://payout.example.com')
  const payoutDailyLimit = page.getByRole('textbox', { name: label('payoutDailyLimit'), exact: true })
  const payoutSigningKey = page.getByLabel(label('payoutSigningKey'), { exact: true })
  await payoutDailyLimit.fill('50000')
  await payoutSigningKey.fill('short')
  await page.getByRole('checkbox', { name: label('enablePayout'), exact: true }).check()
  await page.getByRole('checkbox', { name: label('autoApprovePayout'), exact: true }).check()
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.getByText(label('validation.dailyLimitBelowMaximum'), { exact: true }).waitFor()
  await page.getByText(label('validation.signingKeyBytes').replace('{{min}}', '32').replace('{{max}}', '1024'), { exact: true }).waitFor()
  assert.equal(payoutSettingsRequests.length, 0, 'Invalid payout settings never reach the server')
  await payoutDailyLimit.fill('1000000')
  await payoutSigningKey.fill('ci-private-signing-key-0123456789')
  await page.getByText(label('validation.dailyLimitBelowMaximum'), { exact: true }).waitFor({ state: 'hidden' })
  await page.getByText(label('validation.signingKeyBytes').replace('{{min}}', '32').replace('{{max}}', '1024'), { exact: true }).waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.getByText(label('saveError'), { exact: true }).waitFor()
  assert.equal(await payoutSigningKey.inputValue(), 'ci-private-signing-key-0123456789')
  assert.equal(payoutSettingsRequests.length, 1)
  assert.equal(payoutSettingsRequests[0].daily_limit_minor, 1000000)
  await page.getByRole('button', { name: label('save'), exact: true }).click()
  await page.waitForFunction(() => !document.querySelector('input[type="password"]')?.value)
  assert.equal(payoutSettingsRequests.length, 2)
  assert.equal(payoutSettingsRequests[1].auto_approve_enabled, true)
  await page.getByRole('button', { name: label('adminTabs.withdrawals'), exact: true }).click()
  await page.getByRole('button', { name: label('queuePayout'), exact: true }).click()
  await page.getByRole('button', { name: label('reconcilePayout'), exact: true }).waitFor()
  assert.equal(await page.getByRole('button', { name: label('reviewWithdrawal'), exact: true }).count(), 0)
  assert.equal(await page.getByRole('button', { name: label('queuePayout'), exact: true }).count(), 0)
  const reconcileResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/api/admin/merchant-withdrawals/5/reconcile' && response.request().method() === 'POST')
  await page.getByRole('button', { name: label('reconcilePayout'), exact: true }).click()
  await reconcileResponse
  assert.equal(payoutSubmissions, 1)
  assert.equal(payoutQueries, 1)
  await page.screenshot({ path: resolve(screenshots, 'merchant-payouts.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('dark'); document.documentElement.classList.add('light') })
  await page.screenshot({ path: resolve(screenshots, 'merchant-payouts-light.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('light'); document.documentElement.classList.add('dark') })
  assert.equal(await page.evaluate(() => [...Object.values(sessionStorage), ...Object.values(localStorage)].some(value => value.includes('ci-private-receiving-token') || value.includes('ci-private-signing-key'))), false)
  await page.getByRole('button', { name: 'key', exact: true }).click()
  await page.getByRole('textbox', { name: commonLabel('apiKeys.fields.name'), exact: true }).fill('CI merchant key')
  await page.getByRole('button', { name: commonLabel('apiKeys.actions.create'), exact: true }).click()
  await page.getByTestId('issued').waitFor()
  await page.setViewportSize({ width: 390, height: 844 })
  await page.getByRole('button', { name: 'catalog', exact: true }).click()
  await page.getByRole('heading', { name: label('market.title'), exact: true }).waitFor()
  const mobileTable = page.getByRole('region', { name: label('catalogTable'), exact: true })
  const mobileGeometry = await mobileTable.evaluate(node => ({ viewport: innerWidth, rootRight: document.documentElement.getBoundingClientRect().right, bodyWidth: document.body.clientWidth, bodyScrollWidth: document.body.scrollWidth, scrollWidth: node.scrollWidth, clientWidth: node.clientWidth }))
  const mobileDocument = await page.evaluate(documentGeometry)
  if (mobileGeometry.rootRight > mobileGeometry.viewport + 1 || mobileGeometry.bodyScrollWidth > mobileGeometry.bodyWidth + 1 || mobileDocument.visibleOverflow.length) console.error('Merchant mobile overflow geometry', JSON.stringify(mobileDocument))
  assert.ok(mobileGeometry.scrollWidth > mobileGeometry.clientWidth, 'Mobile merchant rows scroll within their table')
  assert.ok(mobileGeometry.rootRight <= mobileGeometry.viewport + 1, 'The mobile document root must remain inside the viewport')
  assert.ok(mobileGeometry.bodyScrollWidth <= mobileGeometry.bodyWidth + 1, 'The mobile page body must not overflow horizontally')
  assert.deepEqual(mobileDocument.visibleOverflow, [], 'Visible page content must remain inside the mobile viewport')
  assert.equal(await mobileTable.locator('tbody tr').count(), 3)
  assert.equal(await mobileTable.getByText(publicService.description, { exact: true }).isVisible(), true, 'Merchant introductions remain visible on mobile')
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-mobile.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('dark'); document.documentElement.classList.add('light') })
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-mobile-light.png'), fullPage: true, animations: 'disabled' })
  await page.evaluate(() => { document.documentElement.classList.remove('light'); document.documentElement.classList.add('dark') })
  await mobileTable.evaluate(node => { node.scrollLeft = node.scrollWidth })
  const fixedMerchant = await mobileTable.locator('tbody tr').first().locator('td').first().boundingBox()
  assert.ok(fixedMerchant && fixedMerchant.x >= 0 && fixedMerchant.x < mobileGeometry.viewport, 'Merchant identity remains visible while scrolling')
  await page.screenshot({ path: resolve(screenshots, 'merchant-catalog-mobile-actions.png'), fullPage: true, animations: 'disabled' })
  await page.setViewportSize({ width: 1280, height: 1000 })
  await page.getByText(label('moreFilters'), { exact: true }).click()
  await page.getByRole('combobox', { name: label('protocol'), exact: true }).selectOption('xai_video')
  await page.getByRole('textbox', { name: label('model'), exact: true }).fill('grok-imagine-video')
  await page.getByText('480p: ' + label('pricePerSecond').replace('{{value}}', '0.04'), { exact: true }).waitFor()
  await page.getByText(label('ttftNotApplicable'), { exact: true }).waitFor()
  await page.screenshot({ path: resolve(screenshots, 'merchant-video.png'), fullPage: true, animations: 'disabled' })
  await page.getByRole('textbox', { name: label('search'), exact: true }).fill('CI merchant')
  await page.getByRole('textbox', { name: label('minimumRatio'), exact: true }).fill('0.5')
  const resetResponse = page.waitForResponse(response => {
    const url = new URL(response.url())
    return url.pathname === '/api/merchant-pools/1/services' && !url.searchParams.get('model') && !url.searchParams.get('search') && !url.searchParams.get('protocol') && !url.searchParams.get('min_ratio_micros')
  })
  await page.getByRole('button', { name: label('market.resetFilters'), exact: true }).click()
  await resetResponse
  assert.equal(await page.getByRole('textbox', { name: label('model'), exact: true }).inputValue(), '')
  assert.equal(await page.getByRole('textbox', { name: label('search'), exact: true }).inputValue(), '')
  const resetQuery = catalogRequests.at(-1)
  for (const key of ['model', 'search', 'protocol', 'min_ratio_micros']) assert.ok(!resetQuery[key], `Reset clears the ${key} filter from the actual catalog request`)
  assert.equal(resetQuery.sort, 'name')
  assert.equal(resetQuery.page, '1')
  await page.getByRole('button', { name: 'admin', exact: true }).click()
  const adminPoolsTable = page.getByRole('region', { name: label('adminTabs.pools'), exact: true })
  const deletablePoolRow = adminPoolsTable.getByRole('row').filter({ hasText: 'CI inherited pool' })
  await deletablePoolRow.getByRole('button', { name: label('delete'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByText(label('deletion.poolHint'), { exact: true }).waitFor()
  assert.equal(poolDeleteRequests.length, 0, 'Opening the confirmation does not delete a pool')
  await dialog.getByRole('button', { name: label('cancel'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  await deletablePoolRow.getByRole('button', { name: label('delete'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('button', { name: label('delete'), exact: true }).click()
  await dialog.getByText(label('deletion.inUse'), { exact: true }).waitFor()
  assert.equal(await dialog.getByText('CI inherited pool', { exact: true }).isVisible(), true, 'A blocked deletion retains the chosen pool')
  await dialog.getByRole('button', { name: label('delete'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  await adminPoolsTable.getByText('CI inherited pool', { exact: true }).waitFor({ state: 'hidden' })
  assert.deepEqual(poolDeleteRequests, [2, 2])
  await page.getByRole('button', { name: label('adminTabs.merchants'), exact: true }).click()
  const adminMerchantsTable = page.getByRole('region', { name: label('adminTabs.merchants'), exact: true })
  await adminMerchantsTable.getByRole('row').filter({ hasText: profile.name }).getByRole('button', { name: label('delete'), exact: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByText(label('deletion.merchantHint'), { exact: true }).waitFor()
  await dialog.getByRole('button', { name: label('delete'), exact: true }).click()
  await dialog.getByText(label('deletion.inUse'), { exact: true }).waitFor()
  await dialog.getByRole('button', { name: label('delete'), exact: true }).click()
  await dialog.waitFor({ state: 'hidden' })
  await adminMerchantsTable.waitFor({ state: 'hidden' })
  assert.deepEqual(merchantDeleteRequests, [6, 6])
  assert.deepEqual(errors, [])
  console.log('Merchant pool browser regression passed')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ errors, poolRequests, serviceRequestCount: serviceRequests.length, quoteRequests, routeRequests, withdrawalRequests, geometry: page ? await page.evaluate(documentGeometry).catch(() => undefined) : undefined, ariaSnapshot: page ? await page.locator('body').ariaSnapshot().catch(() => undefined) : undefined }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
