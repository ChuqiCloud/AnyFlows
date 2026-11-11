import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

// Vite, browser caches, fixtures and screenshots are created on the remote CI runner only.
assert.equal(process.env.CI, 'true', 'Navigation browser checks run in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Use the remote Playwright installation')
assert.ok(process.env.RUNNER_TEMP, 'Screenshots stay on the remote CI runner')
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const translations = Object.fromEntries(await Promise.all(['zh', 'en'].map(async language => [language, JSON.parse(await readFile(resolve(directory, `src/i18n/locales/${language}.json`), 'utf8'))])))
const publicStatusCopy = JSON.parse(await readFile(resolve(directory, 'src/i18n/public-status.json'), 'utf8')).zh.publicStatus
const label = (key, language = 'zh') => {
  const value = key.split('.').reduce((node, part) => node?.[part], translations[language])
  assert.equal(typeof value, 'string', `Missing ${language} translation: ${key}`)
  return value
}
const html = resolve(directory, '.navigation-smoke.html')
const entry = resolve(directory, '.navigation-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'navigation-screenshots-web-next')
const port = 4174
const origin = `http://127.0.0.1:${port}`
const errors = []
const requests = []
const model = {
  model: 'ci-catalog-chat', display_name: 'CI Catalog Chat', provider: 'xiaomi-mimo',
  description: null, icon_url: null, tags: [], context_window: 131072,
  input_modalities: ['text'], output_modalities: ['text'], supports_reasoning: false,
  supports_tool_calls: true, supports_responses_compact: false, lifecycle: 'active',
  runtime_status: 'available', billing_mode: 'per_token',
  prices: { input: '1.23004567890123456789', output: '9.87654321', cache_read: '0', cache_creation_5m: '0', cache_creation_1h: '0' },
  ratios: { group_micros: '1000000', group_model_micros: '1000000', peak_micros: '1000000' },
  price_version: 1, available_protocols: ['openai_chat', 'openai_responses', 'anthropic'],
}
const site = {
  site_name: 'CI AnyFlows', public_base_url: 'https://gateway.example.test/',
  brand: { logo_url: null, tagline: '', description: '' },
  navigation: { header_links: [], footer_groups: [], sidebar_links: [] },
  balance_display: { mode: 'quota', unit_name: '算力积分', unit_symbol: '积分', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 },
  authentication: { password_login_enabled: true, registration_enabled: false, registration_email_required: false, oauth_providers: [], turnstile_site_key: null },
}
const subscriptionPlan = {
  plan_id: 'ci-subscription-plan', name: 'CI subscription plan', quota_amount: 100000,
  cycle: 'monthly', plan_version: 1, price_provider: 'epay', price_currency: 'CNY', price_amount_minor: 1000,
}
const terminalSubscriptionPlan = {
  plan_id: 'ci-terminal-subscription-plan', name: 'CI terminal subscription plan', quota_amount: 200000,
  cycle: 'monthly', plan_version: 1, price_provider: 'stripe', price_currency: 'USD', price_amount_minor: 2000,
}
const groupWindow = { usage: 0, started_at: 1700000000, resets_at: 1700086400 }
const group = {
  id: 1, name: 'ci-group', display_name: 'CI group', ratio_micros: 1000000,
  peak_ratio_micros: null, peak_start: null, peak_end: null, peak_windows: null,
  is_exclusive: false, daily_limit: null, weekly_limit: null, monthly_limit: null,
  daily_window: groupWindow, weekly_window: groupWindow, monthly_window: groupWindow,
  rpm_limit: null, fallback_group_id: null, flags: {},
}
const subscriptionCreateRequests = []
let adminGroups = [group, { ...group, id: 2, name: 'ci-replacement', display_name: 'CI replacement group' }, { ...group, id: 3, name: 'ci-pool-group', display_name: 'CI merchant pool group' }, { ...group, id: 4, name: 'ci-retired-pool-group', display_name: 'CI retired merchant pool group' }]
const groupDeleteRequests = []
let groupTransferFailures = 1
let terminalSubscriptionOrderStatus = 'failed'
let modelMode = 'ready'
let publicStatusMode = 'operational'
let issuedApiKeyRequests = []
let server
let browser
let page
let serverOutput = ''
let releasePlaygroundRequest
let playgroundFailureMode = false

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Navigation CI</title><div id="root"></div><script type="module" src="/.navigation-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter } from 'react-router-dom';
    import { Provider } from './src/provider';
    import { QueryProvider } from './src/lib/query';
    import { appQueryClient } from './src/lib/query/query-client';
    import { RouterNavigationBridge } from './src/lib/router-navigation';
    import { App } from './src/App';
    import './src/index.css';
    import './src/i18n';
    const path = new URLSearchParams(window.location.search).get('path') || '/';
    window.history.replaceState(null, '', path);
    appQueryClient.setDefaultOptions({ queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } });
    createRoot(document.getElementById('root')!).render(
      <BrowserRouter><Provider><QueryProvider><RouterNavigationBridge /><App /></QueryProvider></Provider></BrowserRouter>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try { if ((await fetch(`${origin}/.navigation-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => {
    const params = new URLSearchParams(window.location.search)
    const role = params.get('role') || 'public'
    localStorage.setItem('anyflows.locale', params.get('language') || 'zh')
    localStorage.removeItem('auth_principal')
    sessionStorage.removeItem('auth_principal')
    if (role === 'public') sessionStorage.removeItem('anyflows.management.access-token')
    else sessionStorage.setItem('anyflows.management.access-token', `ci-${role}`)
  })
  // AnyFlows has no platform WebSocket endpoint; keep its optional reconnect probe local.
  await page.route(url => url.pathname === '/ws/token', route => route.fulfill({
    status: 404, contentType: 'application/json', body: '{"code":"not_found"}',
  }))
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    const token = request.headers().authorization || ''
    const role = token.includes('ci-admin') ? 'admin' : token.includes('ci-user') ? 'user' : 'public'
    requests.push({ path, role })
    if (path === '/api/admin/groups/1' && request.method() === 'DELETE' && role === 'admin') {
      const query = new URL(request.url()).searchParams
      groupDeleteRequests.push(Object.fromEntries(query))
      if (!query.has('replacement_group_id')) return route.fulfill({ status: 409, contentType: 'application/json', body: JSON.stringify({ code: 'group_in_use' }) })
      assert.equal(query.get('replacement_group_id'), '2', 'An explicit destination is required for user transfer')
      if (groupTransferFailures-- > 0) return route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ code: 'temporary_error' }) })
      adminGroups = adminGroups.filter(item => item.id !== 1)
      return route.fulfill({ status: 204 })
    }
    if (path === '/api/playground/v1/chat/completions' && request.method() === 'POST' && role === 'user') {
      assert.equal(request.postDataJSON().model, model.model)
      if (playgroundFailureMode) {
        playgroundFailureMode = false
        await route.fulfill({ status: 401, headers: { 'x-request-id': 'ci-playground-request' }, contentType: 'application/json', body: JSON.stringify({ error: { message: 'ci-admin-trace-must-not-display', code: 'invalid_api_key' } }) })
        return
      }
      // Hold the response until the real composer has entered its sending state.
      // Clicking Stop must abort the browser request without calling any upstream.
      await new Promise(resolve => { releasePlaygroundRequest = resolve })
      await route.abort('aborted').catch(() => {})
      return
    }
    if (path === '/api/tokens' && request.method() === 'POST' && role === 'user') {
      const body = request.postDataJSON()
      issuedApiKeyRequests.push(body)
      const now = Math.floor(Date.now() / 1000)
      await route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify({
        api_key: 'ci-issued-secret-must-not-appear-in-samples',
        token: { id: 42, group_id: body.group_id ?? null, user_route_id: body.user_route_id ?? null, effective_group_id: 1, group_display_name: 'CI group', key_prefix: 'sk-ci-', name: body.name, status: body.status, remain_quota: body.remain_quota, unlimited_quota: body.unlimited_quota, used_quota: 0, expired_at: body.expired_at ?? null, model_limits: body.model_limits ?? null, allow_ips: body.allow_ips ?? null, created_at: now, updated_at: now },
      }) })
      return
    }
    if (path === '/api/explorer/catalog' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ operations: [{ operation_id: 'chatCompletions', method: 'POST', path: '/v1/chat/completions', tag: 'OpenAI Chat', summary: 'OpenAI Chat Completions', auth_mode: 'api_key', access_scope: 'gateway', debuggable: true }], tags: ['OpenAI Chat'], total: 1, page: 1, page_size: 60, has_more: false, viewer: role === 'public' ? 'guest' : role }) })
      return
    }
    if (path === '/api/explorer/catalog/chatCompletions' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ operation: { operation_id: 'chatCompletions', method: 'POST', path: '/v1/chat/completions', tag: 'OpenAI Chat', summary: 'OpenAI Chat Completions', auth_mode: 'api_key', access_scope: 'gateway', debuggable: true }, openapi: { requestBody: { content: { 'application/json': { schema: { type: 'object', properties: { model: { type: 'string' }, messages: { type: 'array', items: { type: 'object', properties: { role: { type: 'string' }, content: { type: 'string' } } } }, max_tokens: { type: 'integer' } } } } } }, responses: { 200: { description: 'success' } } } }) })
      return
    }
    if (path === '/api/account/subscription-orders' && request.method() === 'POST' && role === 'user') {
      const body = request.postDataJSON()
      const { idempotency_key, ...snapshot } = body
      const { plan_id, plan_version, price_provider, price_currency, price_amount_minor } = subscriptionPlan
      assert.deepEqual(snapshot, { plan_id, plan_version, price_provider, price_currency, price_amount_minor })
      assert.ok(typeof idempotency_key === 'string' && idempotency_key.length > 0, 'Creating a subscription supplies an idempotency key')
      subscriptionCreateRequests.push(body)
      // Fail before an order exists, including on the explicit retry. No payment
      // session endpoint should be called after either failed create attempt.
      await route.fulfill({ status: 503, contentType: 'application/json', body: '{"code":"topup_unavailable"}' })
      return
    }
    if (path === '/api/account/subscription-orders/ci-terminal-order' && request.method() === 'GET' && role === 'user') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({
        order_id: 'ci-terminal-order', plan_id: terminalSubscriptionPlan.plan_id, plan_version: 1,
        price_provider: 'stripe', price_currency: 'USD', price_amount_minor: 2000, quota_amount: 200000,
        status: terminalSubscriptionOrderStatus, version: 1, expires_at: null, paid_at: null,
        created_at: 1700000000, updated_at: 1700000000, replayed: false,
      }) })
      return
    }
    if (request.method() !== 'GET' || (role !== 'admin' && path.startsWith('/api/admin/'))) {
      errors.push(`Unauthorized or unexpected request: ${request.method()} ${path} (${role})`)
      await route.fulfill({ status: 403, contentType: 'application/json', body: '{"code":"forbidden"}' })
      return
    }
    let data
    switch (path) {
      case '/api/site': data = site; break
      case '/api/setup/status': data = { setup_required: false }; break
      case '/api/announcements': data = { entries: [] }; break
      case '/api/public/service-status': data = publicStatusMode === 'insufficient_data'
        ? { sampled_at: 1700086400, period_start: 1700000000, period_end: 1700086400, status: 'insufficient_data', api_ready: true, sample_count: null, successful_request_count: null, failed_request_count: null, unknown_request_count: null, availability_percent: null, average_success_duration_ms: null, hourly: [] }
        : publicStatusMode === 'monitoring'
          ? { sampled_at: 1700086400, period_start: 1700000000, period_end: 1700086400, status: 'monitoring', api_ready: true, sample_count: 80, successful_request_count: 70, failed_request_count: 2, unknown_request_count: 8, availability_percent: 97.22, average_success_duration_ms: 84, hourly: Array.from({ length: 24 }, (_, index) => ({ period_start: 1700000000 + index * 3600, successful_request_count: 3, failed_request_count: 0, unknown_request_count: index === 11 ? 8 : 0 })) }
          : { sampled_at: 1700086400, period_start: 1700000000, period_end: 1700086400, status: 'operational', api_ready: true, sample_count: 80, successful_request_count: 79, failed_request_count: 1, unknown_request_count: 0, availability_percent: 98.75, average_success_duration_ms: 84, hourly: Array.from({ length: 24 }, (_, index) => ({ period_start: 1700000000 + index * 3600, successful_request_count: 3, failed_request_count: index === 11 ? 1 : 0, unknown_request_count: 0 })) }
        ; break
      case '/api/auth/session': data = { user: { id: 1, role }, expires_at: Math.floor(Date.now() / 1000) + 3600 }; break
      case '/api/models':
        if (modelMode === 'error') {
          await route.fulfill({ status: 503, contentType: 'application/json', body: '{"code":"catalog_unavailable"}' })
          return
        }
        data = { pricing_scope: role === 'public' ? 'public_base' : 'group', next_cursor: null, models: modelMode === 'empty' ? [] : [model] }
        break
      case '/api/model-providers': data = { pricing_scope: role === 'public' ? 'public_base' : 'group', providers: [{ name: model.provider, model_count: 1 }] }; break
      case '/api/model-provider-catalog':
      case '/api/admin/model-provider-catalog': data = { providers: [] }; break
      case '/api/account/profile': data = { id: 1, username: 'CI user', email: null, role, notifications: {} }; break
      case '/api/account/notifications': data = { entries: [], next_cursor: null, unread_count: 0 }; break
      case '/api/account/verifications/eligibility': data = { verified: false, require_verification_for_topup: false, require_verification_for_api: false, enterprise_verified: false, can_apply_for_organization: false, providers: [], individual_providers: [], enterprise_providers: [], individual_reason_required: true, enterprise_reason_required: true }; break
      case '/api/account/wallet': data = { balance: 100000, used_quota: 0, frozen_quota: 0 }; break
      case '/api/account/wallet/entries':
      case '/api/account/refund-reconciliations': data = { entries: [], next_cursor: null }; break
      case '/api/account/subscriptions': data = { subscriptions: [], next_cursor: null }; break
      case '/api/account/subscription-catalog': data = { plans: [subscriptionPlan, terminalSubscriptionPlan] }; break
      case '/api/account/wallet/topups/config':
        data = { methods: [
          { provider: 'epay', payment_method: 'alipay', currency: 'CNY', min_amount_minor: 50, max_amount_minor: 99999999, qr_enabled: false },
          { provider: 'stripe', payment_method: 'card', currency: 'USD', min_amount_minor: 50, max_amount_minor: 99999999, publishable_key: null, qr_enabled: false },
        ] }
        break
      case '/api/tokens': data = { tokens: [], next_cursor: null, capacity: 32 }; break
      case '/api/user-routes': data = { items: [], capacity: 32 }; break
      case '/api/token-groups': data = { groups: [], next_cursor: null }; break
      case '/api/account/usage-logs': data = { logs: [], failed_logs: [], next_cursor: null, failed_next_cursor: null }; break
      case '/api/admin/tokens': data = { tokens: [], next_cursor: null }; break
      case '/api/admin/groups': data = { groups: adminGroups, next_cursor: null }; break
      case '/api/admin/merchant-pools': data = { items: new URL(request.url()).searchParams.get('status') === 'deleted' ? [{ id: 8, group_id: 4 }] : [{ id: 7, group_id: 3 }], page: 1, page_size: 100, total: 1 }; break
      case '/api/admin/groups/1/key-policy': data = { group_id: 1, selection_mode: 'default_only', allowed_user_ids: [], allowed_group_ids: [] }; break
      case '/api/admin/groups/1/model-policy': data = { group_id: 1, allowed_models: null, ratio_overrides: [{ source_group_id: 1, model: '*', ratio_micros: 1000000 }] }; break
      case '/api/admin/models': data = { models: [], next_cursor: null }; break
      case '/api/admin/software':
        data = { edition: 'community', version: '0.3.0-beta.9', core_version: '0.3.0-beta.9', target: 'x86_64-unknown-linux-gnu', deployment: 'binary', node_id: 'ci-navigation', launch_mode: 'launcher', launcher_protocol: 1, launcher_version: '0.3.0-beta.9', update_phase: 'idle', check_enabled: true, install_enabled: true, restart_required: false, update_available: false }
        break
      default:
        errors.push(`Unexpected API: ${path} (${role})`)
        await route.fulfill({ status: 404, contentType: 'application/json', body: '{}' })
        return
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(data) })
  })
  const open = async (path = '/', role = 'public') => {
    await page.goto(`${origin}/.navigation-smoke.html?${new URLSearchParams({ path, role })}`)
  }
  const noOverflow = async (description) => {
    await page.evaluate(() => document.fonts.ready)
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), description)
  }
  const visibleNav = () => page.locator(`nav[aria-label="${label('nav.label')}"]:visible`)
  // A section and its first leaf can intentionally share a translated title (for example, users).
  const item = key => visibleNav().getByRole('option', { name: label(`nav.${key}`), exact: true }).last()
  const expand = async key => {
    const trigger = visibleNav().getByRole('button', { name: label(`nav.sections.${key}`), exact: true })
    await trigger.waitFor()
    if (await trigger.getAttribute('aria-expanded') !== 'true') await trigger.click()
  }

  // The landing page is a static marketing page: it must render its featured-model table
  // and the deployment URL without touching the model catalog API.
  const featuredModel = 'Claude Opus 5.5'
  await open()
  await page.getByRole('table').getByText(featuredModel, { exact: true }).waitFor()
  await page.getByText('"https://gateway.example.test/v1"', { exact: true }).waitFor()
  await page.getByText(publicStatusCopy.states.operational, { exact: true }).waitFor()
  assert.equal(await page.getByText(`${publicStatusCopy.availability}: 98.75%`, { exact: true }).count(), 1, 'Landing uses the real public status aggregate rather than fixture marketing data')
  const landingText = await page.locator('main').innerText()
  assert.equal(landingText.includes('99.98%'), false, 'Landing does not claim a static availability guarantee')
  assert.equal(landingText.includes('82ms'), false, 'Landing does not display fabricated latency measurements')
  const gradientIds = await page.locator('rect.animate-border-beam').evaluateAll(elements => elements.map(element => element.getAttribute('stroke')))
  assert.equal(new Set(gradientIds).size, gradientIds.length, 'SVG gradients have unique IDs')
  await page.getByRole('button', { name: label('language.toggle'), exact: true }).filter({ visible: true }).click()
  await page.getByRole('table').getByText(featuredModel, { exact: true }).waitFor()
  assert.equal(await page.locator('html').getAttribute('lang'), 'en')
  await page.getByRole('button', { name: label('language.toggle', 'en'), exact: true }).filter({ visible: true }).click()
  for (const theme of ['dark', 'light']) {
    await page.evaluate(mode => {
      document.documentElement.classList.remove('dark', 'light')
      document.documentElement.classList.add(mode)
    }, theme)
    for (const width of [1280, 390, 320]) {
      await page.setViewportSize({ width, height: width === 1280 ? 960 : 844 })
      await noOverflow(`Landing ${theme} overflows at ${width}px`)
      if (width !== 320) await page.screenshot({ path: resolve(screenshots, `landing-${theme}-${width}.png`), fullPage: true, animations: 'disabled' })
    }
  }
  await page.setViewportSize({ width: 1280, height: 960 })
  assert.ok(!requests.some(request => request.role === 'public' && request.path.startsWith('/api/models')), 'Landing never fetches the model catalog')

  await page.getByText(publicStatusCopy.history, { exact: true }).click()
  await page.getByRole('heading', { name: publicStatusCopy.title, exact: true }).waitFor()
  await page.getByText(publicStatusCopy.historyHint, { exact: true }).waitFor()
  assert.equal(await page.getByText('98.75%', { exact: true }).count(), 1)
  assert.equal((await page.locator('main').innerText()).includes('private-model-name-is-not-in-the-public-report'), false, 'The public status page must not expose model identifiers')
  publicStatusMode = 'monitoring'
  await page.getByRole('button', { name: publicStatusCopy.refresh, exact: true }).click()
  await page.getByText(publicStatusCopy.states.monitoring, { exact: true }).waitFor()
  await page.getByText(publicStatusCopy.monitoring, { exact: true }).waitFor()
  publicStatusMode = 'insufficient_data'
  await page.getByRole('button', { name: publicStatusCopy.refresh, exact: true }).click()
  await page.getByText(publicStatusCopy.states.insufficient_data, { exact: true }).waitFor()
  assert.equal(await page.getByText('98.75%', { exact: true }).count(), 0, 'Low sample volume never displays an availability rate')
  await page.screenshot({ path: resolve(screenshots, 'public-status.png'), fullPage: true, animations: 'disabled' })

  // Issuing a key opens protocol-aware, credential-placeholder code samples.
  await open('/console/api-keys', 'user')
  await page.getByRole('button', { name: label('apiKeys.actions.create'), exact: true }).click()
  const keyEditor = page.getByRole('dialog')
  await keyEditor.getByRole('textbox', { name: label('apiKeys.fields.name'), exact: true }).fill('CI onboarding key')
  await keyEditor.getByRole('button', { name: label('apiKeys.actions.create'), exact: true }).click()
  await page.getByRole('heading', { name: label('apiKeys.issued.title'), exact: true }).waitFor()
  assert.equal(issuedApiKeyRequests.length, 1)
  const onboarding = page.getByRole('region', { name: label('apiKeys.issued.onboarding.title'), exact: true })
  await onboarding.getByRole('combobox', { name: label('apiKeys.issued.onboarding.model'), exact: true }).waitFor()
  const sample = onboarding.locator('pre')
  await sample.waitFor()
  assert.equal((await sample.innerText()).includes('ci-issued-secret-must-not-appear-in-samples'), false, 'One-time key material is never inserted into code samples')
  assert.ok((await sample.innerText()).includes('<your-api-key>') || (await sample.innerText()).includes('ANYFLOWS_API_KEY'), 'Samples use a placeholder or environment variable')
  await onboarding.getByRole('combobox', { name: label('apiKeys.issued.onboarding.protocol'), exact: true }).selectOption('anthropic')
  await page.waitForFunction(() => document.querySelector('pre')?.innerText.includes('/messages'))
  assert.ok((await sample.innerText()).includes('/messages'))
  await onboarding.getByRole('button', { name: label('apiKeys.issued.onboarding.languages.javascript'), exact: true }).click()
  assert.equal((await sample.innerText()).includes('ANYFLOWS_API_KEY'), true)
  assert.equal((await sample.innerText()).includes('ci-issued-secret-must-not-appear-in-samples'), false)
  await page.getByRole('button', { name: label('apiKeys.issued.done'), exact: true }).click()
  await page.getByRole('heading', { name: label('apiKeys.title'), exact: true }).waitFor()

  await open('/api', 'public')
  await page.getByRole('heading', { name: label('apiExplorer.title'), exact: true }).waitFor()
  const explorerSearch = page.getByPlaceholder(label('apiExplorer.search'))
  await explorerSearch.fill('Chat Completions')
  const chatOperation = page.getByRole('button').filter({ hasText: 'OpenAI Chat Completions' }).first()
  await chatOperation.click()
  const requestBody = page.getByRole('textbox', { name: label('apiExplorer.requestBody'), exact: true })
  await page.waitForFunction(() => [...document.querySelectorAll('textarea')].some((element) => element.value.includes('"messages"')))
  const safeBody = JSON.parse(await requestBody.inputValue())
  assert.deepEqual(safeBody.messages, [{ role: 'user', content: 'Hello' }])
  assert.equal(JSON.stringify(safeBody).includes('ci-issued-secret-must-not-appear-in-samples'), false)
  assert.equal(JSON.stringify(safeBody).includes('api_key'), false, 'OpenAPI examples omit secret fields')
  // The ordinary-user overview must render and administrator URLs must return to it.
  await open('/console', 'user')
  await page.getByRole('heading', { name: label('dashboard.user.title'), exact: true }).waitFor()
  await item('overview').waitFor()
  await item('userRoutes').waitFor()
  await item('userRoutes').click()
  await page.getByRole('heading', { name: label('userRoutes.title'), exact: true }).waitFor()
  assert.equal(new URL(page.url()).pathname, '/console/user-routes')
  assert.equal(await item('userRoutes').getAttribute('aria-selected'), 'true')
  assert.equal(await page.getByRole('button', { name: label('nav.sections.system'), exact: true }).count(), 0)
  await open('/console/tokens', 'user')
  await page.getByRole('heading', { name: label('dashboard.user.title'), exact: true }).waitFor()
  assert.equal(new URL(page.url()).pathname, '/console')
  assert.ok(requests.some(request => request.role === 'user' && request.path === '/api/account/wallet'))
  assert.ok(!requests.some(request => request.role === 'user' && request.path.startsWith('/api/admin/')), 'User overview never calls management APIs')

  // A first create failure must be visible even though no order was returned,
  // and its retry must use the same idempotency key without an unhandled rejection.
  await open('/console/wallet', 'user')
  const subscriptionCatalog = page.getByRole('region', { name: label('subscriptions.catalog.title'), exact: true })
  const subscriptionRow = subscriptionCatalog.getByRole('article').filter({ has: page.getByRole('heading', { name: subscriptionPlan.name, exact: true }) })
  await subscriptionRow.getByRole('button', { name: label('subscriptions.catalog.orderCreate'), exact: true }).click()
  const subscriptionError = subscriptionRow.getByRole('alert')
  await subscriptionError.getByText(label('subscriptions.errors.paymentUnavailable'), { exact: true }).waitFor()
  assert.equal(subscriptionCreateRequests.length, 1, 'Initial create sends exactly one request')
  assert.equal(await subscriptionRow.getByText(label('subscriptions.catalog.orderId'), { exact: true }).count(), 0, 'An unsuccessful create never displays an order')
  const retrySubscription = subscriptionError.getByRole('button', { name: label('subscriptions.catalog.retryPayment'), exact: true })
  assert.ok(await retrySubscription.isEnabled(), 'Initial create failure offers an enabled retry')
  await Promise.all([
    page.waitForResponse(response => new URL(response.url()).pathname === '/api/account/subscription-orders' && response.request().method() === 'POST'),
    retrySubscription.click(),
  ])
  await subscriptionError.getByText(label('subscriptions.errors.paymentUnavailable'), { exact: true }).waitFor()
  assert.equal(subscriptionCreateRequests.length, 2, 'Retry sends another create request')
  assert.equal(subscriptionCreateRequests[1].idempotency_key, subscriptionCreateRequests[0].idempotency_key, 'Retry keeps the original subscription idempotency key')
  assert.equal(await subscriptionRow.getByText(label('subscriptions.catalog.orderId'), { exact: true }).count(), 0, 'Failed retry still has no order')
  await page.screenshot({ path: resolve(screenshots, 'subscription-create-failure.png'), fullPage: true, animations: 'disabled' })

  // A redirect may report processing while the server has already recorded a
  // terminal order. The terminal order state must win and expose a new-order
  // action for failed, canceled, and expired orders.
  await page.evaluate((planId) => {
    sessionStorage.setItem(`anyflows.subscription.payment-attempt:${planId}`, JSON.stringify({
      idempotencyKey: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', orderId: 'ci-terminal-order',
      paymentIntentId: 'ci-terminal-payment', paymentMethod: 'card', planId, provider: 'stripe',
    }))
  }, terminalSubscriptionPlan.plan_id)
  for (const status of ['failed', 'canceled', 'expired']) {
    terminalSubscriptionOrderStatus = status
    await open(`/console/wallet?flow=subscription&subscription_order_id=ci-terminal-order&subscription_payment_intent=ci-terminal-payment&payment_intent=ci-terminal-payment&redirect_status=processing`, 'user')
    const terminalCatalog = page.getByRole('region', { name: label('subscriptions.catalog.title'), exact: true })
    const terminalRow = terminalCatalog.getByRole('article').filter({ has: page.getByRole('heading', { name: terminalSubscriptionPlan.name, exact: true }) })
    const newOrder = terminalRow.getByRole('button', { name: label('subscriptions.catalog.newOrder'), exact: true })
    await newOrder.waitFor()
    await page.waitForFunction(() => !new URL(window.location.href).searchParams.has('flow'))
    assert.equal(await terminalRow.getByText(label('subscriptions.catalog.paymentProcessing'), { exact: true }).count(), 0, `${status} terminal order must not remain in processing state`)
    assert.equal(await terminalRow.getByText(label(`subscriptions.catalog.payment${status[0].toUpperCase()}${status.slice(1)}`), { exact: true }).count(), 1, `${status} terminal order shows its server state`)
    if (status === 'expired') {
      await newOrder.click()
      await terminalRow.getByRole('button', { name: label('subscriptions.catalog.orderCreate'), exact: true }).waitFor()
      assert.equal(subscriptionCreateRequests.length, 2, 'Resetting a terminal order does not automatically create another order')
    }
  }
  await page.screenshot({ path: resolve(screenshots, 'subscription-terminal-order.png'), fullPage: true, animations: 'disabled' })

  // Render the real Playground through App so missing component CSS cannot pass
  // as it could in an isolated fixture with its own layout rules.
  await open('/console/playground', 'user')
  const composer = page.locator('.prompt-composer')
  const textarea = page.getByRole('textbox', { name: label('playground.composer.label'), exact: true })
  const send = composer.getByRole('button', { name: label('playground.actions.send'), exact: true })
  const clearConversation = page.getByRole('button', { name: label('playground.actions.clearConversation'), exact: true })
  await composer.waitFor()
  await composer.getByRole('button', { name: model.model }).waitFor()
  const checkComposerLayout = async description => {
    // Allow resize observers and responsive components to commit before measuring.
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    await noOverflow(description)
    const layout = await composer.evaluate(element => {
      const rect = node => {
        const { x, y, width, height, right, bottom } = node.getBoundingClientRect()
        return { x, y, width, height, right, bottom }
      }
      const toolbar = element.querySelector('.prompt-composer__toolbar')
      const field = element.querySelector('textarea')
      const buttons = [...toolbar.querySelectorAll('button')]
        .filter(button => button.getClientRects().length > 0)
        .map(rect)
      return {
        shell: rect(element), field: rect(field), toolbar: rect(toolbar), buttons,
        overflow: element.scrollWidth > element.clientWidth + 1,
        viewport: { width: innerWidth, height: innerHeight },
      }
    })
    assert.ok(!layout.overflow, `${description}: composer content overflows`)
    assert.ok(layout.field.width >= layout.shell.width - 48, `${description}: textarea is not full width`)
    assert.ok(layout.field.bottom <= layout.toolbar.y + 1, `${description}: textarea overlaps toolbar`)
    assert.ok(layout.shell.x >= -1 && layout.shell.right <= layout.viewport.width + 1, `${description}: composer exceeds viewport width`)
    assert.ok(layout.shell.y >= -1 && layout.shell.bottom <= layout.viewport.height + 1, `${description}: composer is below the viewport`)
    assert.ok(layout.buttons.length >= 2, `${description}: model picker and action must remain visible`)
    for (const [index, button] of layout.buttons.entries()) {
      assert.ok(button.x >= layout.shell.x - 1 && button.right <= layout.shell.right + 1 && button.bottom <= layout.shell.bottom + 1, `${description}: toolbar button exceeds composer bounds`)
      for (const other of layout.buttons.slice(index + 1)) {
        const overlaps = button.x < other.right - 1 && button.right > other.x + 1 && button.y < other.bottom - 1 && button.bottom > other.y + 1
        assert.ok(!overlaps, `${description}: toolbar buttons overlap`)
      }
    }
  }
  for (const theme of ['dark', 'light']) {
    await page.evaluate(mode => {
      document.documentElement.classList.remove('dark', 'light')
      document.documentElement.classList.add(mode)
    }, theme)
    for (const width of [1280, 390, 320]) {
      await page.setViewportSize({ width, height: width === 1280 ? 720 : 844 })
      const description = `Playground ${theme} at ${width}px`
      await checkComposerLayout(description)
      const initialHeight = await textarea.evaluate(element => element.getBoundingClientRect().height)
      await textarea.fill(Array.from({ length: 40 }, (_, index) => `CI line ${index + 1}: long composer content`).join('\n'))
      const longInput = await textarea.evaluate(element => ({
        height: element.getBoundingClientRect().height,
        maxHeight: Number.parseFloat(getComputedStyle(element).maxHeight),
        clientHeight: element.clientHeight,
        scrollHeight: element.scrollHeight,
        overflowY: getComputedStyle(element).overflowY,
      }))
      assert.ok(Number.isFinite(longInput.maxHeight) && longInput.height <= longInput.maxHeight + 2, `${description}: long input has no height limit`)
      assert.ok(longInput.scrollHeight > longInput.clientHeight && ['auto', 'scroll'].includes(longInput.overflowY), `${description}: long input cannot scroll internally`)
      await checkComposerLayout(`${description} with long input`)
      await page.screenshot({ path: resolve(screenshots, `playground-${theme}-${width}.png`), fullPage: true, animations: 'disabled' })
      await clearConversation.click()
      assert.equal(await textarea.inputValue(), '')
      assert.ok(await textarea.evaluate(element => element.getBoundingClientRect().height) <= initialHeight + 2, `${description}: clearing does not restore textarea height`)
      assert.ok(await send.isDisabled(), `${description}: empty composer can send`)
    }
  }
  await textarea.fill('CI pending response')
  await send.click()
  const stop = composer.getByRole('button', { name: label('playground.actions.stopAll'), exact: true })
  await stop.waitFor()
  await page.waitForFunction(() => document.querySelector('.prompt-composer textarea')?.disabled === true)
  await checkComposerLayout('Playground while sending')
  // Wait for the intercepted request as well as the optimistic streaming UI.
  for (let attempt = 0; attempt < 100 && !releasePlaygroundRequest; attempt += 1) await delay(50)
  assert.ok(releasePlaygroundRequest, 'Sending reaches the session-authenticated Playground endpoint')
  await stop.click()
  await stop.waitFor({ state: 'hidden' })
  assert.ok(await textarea.isEnabled(), 'Stopping restores the composer')
  assert.equal(await textarea.inputValue(), '')
  await checkComposerLayout('Playground after stopping')
  releasePlaygroundRequest()
  releasePlaygroundRequest = undefined
  await page.evaluate(() => new Promise(resolve => setTimeout(resolve, 0)))
  await clearConversation.click()
  playgroundFailureMode = true
  await textarea.fill('CI invalid key diagnostic')
  await send.click()
  const playgroundAlert = page.getByRole('alert').filter({ hasText: label('playground.diagnostics.invalid_key') })
  await playgroundAlert.waitFor()
  assert.equal(await page.evaluate(() => sessionStorage.getItem('anyflows.management.access-token')), 'ci-user', 'A downstream API Key failure does not invalidate the user session')
  assert.equal((await playgroundAlert.innerText()).includes('ci-admin-trace-must-not-display'), false, 'User diagnostics do not expose backend error details')
  const manageKeysLink = playgroundAlert.getByRole('link', { name: label('playground.diagnostics.manageKeys'), exact: true })
  assert.equal(new URL(await manageKeysLink.getAttribute('href'), page.url()).pathname, '/console/api-keys', 'Invalid-key guidance links to a user-accessible repair action')
  await clearConversation.click()
  await page.setViewportSize({ width: 1280, height: 960 })

  // Use actual keyboard events against the real editor. Keeping an element
  // handle makes remounting the input detectable even if focus were restored.
  await open('/console/system-settings/groups', 'admin')
  await page.getByRole('heading', { name: label('groups.title'), exact: true }).waitFor()
  await page.getByRole('row').filter({ hasText: group.display_name }).getByRole('button', { name: label('groups.actions.edit'), exact: true }).click()
  const groupEditor = page.getByRole('dialog')
  await groupEditor.getByRole('button', { name: label('groups.modelPolicy.tab'), exact: true }).click()
  const ratioRules = groupEditor.getByRole('region', { name: label('groups.modelPolicy.ratioTitle'), exact: true })
  const typeRatioModel = async (input, value) => {
    await input.click()
    const original = await input.elementHandle()
    assert.ok(original, 'The model input is mounted before typing')
    try {
      await page.keyboard.type(value, { delay: 25 })
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      assert.ok(await original.evaluate(element => element.isConnected && document.activeElement === element), 'Editing the ratio model preserves the same focused input')
      assert.equal(await input.inputValue(), value, 'Every typed character is retained')
    } finally {
      await original.dispose()
    }
  }
  await typeRatioModel(ratioRules.getByRole('textbox', { name: label('groups.modelPolicy.ratioModel'), exact: true }), 'ci-existing-model')
  await ratioRules.getByRole('button', { name: label('groups.modelPolicy.ratioAdd'), exact: true }).click()
  await typeRatioModel(ratioRules.getByRole('textbox', { name: label('groups.modelPolicy.ratioModel'), exact: true }).nth(1), 'ci-new-model')
  assert.equal(await ratioRules.getByRole('textbox', { name: label('groups.modelPolicy.ratioModel'), exact: true }).first().inputValue(), 'ci-existing-model', 'Adding and editing a rule retains the previous model')
  await page.screenshot({ path: resolve(screenshots, 'group-ratio-input-focus.png'), fullPage: true, animations: 'disabled' })

  await open('/console/system-settings/groups', 'admin')
  const sourceGroupRow = page.getByRole('row').filter({ hasText: group.display_name })
  await sourceGroupRow.getByRole('button', { name: label('groups.actions.delete'), exact: true }).click()
  const deleteGroupDialog = page.getByRole('dialog')
  const replacementGroup = deleteGroupDialog.getByRole('combobox', { name: label('groups.delete.replacement'), exact: true })
  await replacementGroup.getByRole('option', { name: 'CI replacement group', exact: true }).waitFor({ state: 'attached' })
  await replacementGroup.selectOption('')
  assert.equal(await replacementGroup.getByRole('option', { name: group.display_name, exact: true }).count(), 0, 'A group cannot migrate users to itself')
  assert.equal(await replacementGroup.getByRole('option', { name: 'CI merchant pool group', exact: true }).count(), 0, 'Merchant-pool groups cannot receive transferred users')
  assert.equal(await replacementGroup.getByRole('option', { name: 'CI retired merchant pool group', exact: true }).count(), 0, 'Deleted pools retain their group isolation')
  await deleteGroupDialog.getByRole('button', { name: label('groups.actions.delete'), exact: true }).click()
  await deleteGroupDialog.getByText(label('groups.errors.group_in_use'), { exact: true }).waitFor()
  await replacementGroup.selectOption('2')
  await deleteGroupDialog.getByText(label('groups.errors.group_in_use'), { exact: true }).waitFor({ state: 'hidden' })
  await deleteGroupDialog.getByText(label('groups.delete.transferHint').replace('{{name}}', 'CI replacement group'), { exact: true }).waitFor()
  await deleteGroupDialog.getByRole('button', { name: label('groups.actions.delete'), exact: true }).click()
  await deleteGroupDialog.getByText(label('groups.errors.unknown'), { exact: true }).waitFor()
  assert.equal(await replacementGroup.inputValue(), '2', 'A failed delete retains the selected destination')
  await deleteGroupDialog.getByRole('button', { name: label('groups.actions.delete'), exact: true }).click()
  await deleteGroupDialog.waitFor({ state: 'hidden' })
  await sourceGroupRow.waitFor({ state: 'hidden' })
  assert.deepEqual(groupDeleteRequests, [{}, { replacement_group_id: '2' }, { replacement_group_id: '2' }])

  // Deep links select the system section; all existing administrator groups remain reachable.
  await open('/console/tokens', 'admin')
  await page.getByRole('heading', { name: label('tokens.title'), exact: true }).waitFor()
  await page.getByRole('button', { name: label('nav.sections.system'), exact: true }).waitFor()
  assert.equal(await item('keys').getAttribute('aria-selected'), 'true')
  await expand('gateway')
  for (const key of ['channels', 'clientProfiles', 'credentials', 'modelSettings', 'modelProviders']) assert.ok(await item(key).isVisible(), `Missing ${key}`)
  await item('modelProviders').click()
  await page.getByRole('heading', { name: '模型厂商目录', exact: true }).waitFor()
  assert.equal(new URL(page.url()).pathname, '/console/system-settings/model-providers')
  assert.equal(await item('modelProviders').getAttribute('aria-selected'), 'true')
  await expand('users')
  for (const key of ['users', 'accountVerificationReviews', 'invitationSettings']) assert.ok(await item(key).isVisible(), `Missing ${key}`)
  await expand('site')
  assert.ok(await item('about').isVisible())
  await item('about').click()
  await page.getByRole('heading', { name: label('about.companyName'), exact: true }).waitFor()
  assert.equal(new URL(page.url()).pathname, '/console/system-settings/about')
  assert.equal(await item('about').getAttribute('aria-selected'), 'true')
  await page.screenshot({ path: resolve(screenshots, 'admin-system-desktop.png'), fullPage: true, animations: 'disabled' })

  // Mobile uses the same tree in a drawer and closes it after navigation.
  await page.setViewportSize({ width: 390, height: 844 })
  await page.getByRole('button', { name: label('shell.expand'), exact: true }).click()
  await visibleNav().waitFor()
  await expand('gateway')
  await item('modelProviders').click()
  await page.getByRole('heading', { name: '模型厂商目录', exact: true }).waitFor()
  await visibleNav().waitFor({ state: 'hidden' })
  await noOverflow('Mobile console overflows at 390px')
  await page.getByRole('button', { name: label('shell.expand'), exact: true }).click()
  await visibleNav().waitFor()
  await item('keys').click()
  await page.getByRole('heading', { name: label('tokens.title'), exact: true }).waitFor()
  await visibleNav().waitFor({ state: 'hidden' })
  await page.screenshot({ path: resolve(screenshots, 'admin-system-mobile.png'), fullPage: true, animations: 'disabled' })
  assert.deepEqual(errors, [])
  console.log('Navigation browser checks passed for web-next: live public status and low-sample suppression, landing, language, user overview, subscription create failure and retry, terminal subscription orders after redirect, Playground composer layout and cancellation, group ratio input focus, administrator groups, and mobile drawer.')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors, requests }))
  throw error
} finally {
  releasePlaygroundRequest?.()
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
