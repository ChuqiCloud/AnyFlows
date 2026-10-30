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
  price_version: 1, available_protocols: ['openai_chat'],
}
const site = {
  site_name: 'CI AnyFlows', public_base_url: 'https://gateway.example.test/',
  brand: { logo_url: null, tagline: '', description: '' },
  navigation: { header_links: [], footer_groups: [], sidebar_links: [] },
  balance_display: { mode: 'quota', unit_name: '算力积分', unit_symbol: '积分', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 },
  authentication: { password_login_enabled: true, registration_enabled: false, registration_email_required: false, oauth_providers: [], turnstile_site_key: null },
}
let modelMode = 'ready'
let server
let browser
let page
let serverOutput = ''

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
  page.on('pageerror', error => errors.push(error.message))
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
      case '/api/account/wallet': data = { balance: 100000, used_quota: 0, frozen_quota: 0 }; break
      case '/api/tokens': data = { tokens: [], next_cursor: null, capacity: 32 }; break
      case '/api/account/usage-logs': data = { logs: [], failed_logs: [], next_cursor: null, failed_next_cursor: null }; break
      case '/api/admin/tokens': data = { tokens: [], next_cursor: null }; break
      case '/api/admin/software':
        data = { edition: 'community', version: '0.3.0-beta.6', core_version: '0.3.0-beta.6', target: 'x86_64-unknown-linux-gnu', deployment: 'binary', node_id: 'ci-navigation', launch_mode: 'launcher', launcher_protocol: 1, launcher_version: '0.3.0-beta.6', update_phase: 'idle', check_enabled: true, install_enabled: true, restart_required: false, update_available: false }
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

  // The ordinary-user overview must render and administrator URLs must return to it.
  await open('/console', 'user')
  await page.getByRole('heading', { name: label('dashboard.user.title'), exact: true }).waitFor()
  await item('overview').waitFor()
  assert.equal(await page.getByRole('button', { name: label('nav.sections.system'), exact: true }).count(), 0)
  await open('/console/tokens', 'user')
  await page.getByRole('heading', { name: label('dashboard.user.title'), exact: true }).waitFor()
  assert.equal(new URL(page.url()).pathname, '/console')
  assert.ok(requests.some(request => request.role === 'user' && request.path === '/api/account/wallet'))
  assert.ok(!requests.some(request => request.role === 'user' && request.path.startsWith('/api/admin/')), 'User overview never calls management APIs')

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
  console.log('Navigation browser checks passed for web-next: catalog pricing and recovery, language, user overview, administrator groups, and mobile drawer.')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors, requests }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
