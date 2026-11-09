import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'User route browser checks run in remote CI')
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
const html = resolve(directory, '.user-routes-smoke.html')
const entry = resolve(directory, '.user-routes-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'user-routes-screenshots-web-next')
const port = 4175
const origin = `http://127.0.0.1:${port}`
const errors = []
const requests = []
const groups = [
  { id: 1, display_name: 'CI low price', ratio_micros: 500000, is_default: true },
  { id: 2, display_name: 'CI stable', ratio_micros: 1000000, is_default: false },
]
let routes = []
let routePostFailures = 1
let tokenPostFailures = 1
const routeId = 21
let issued
let server
let browser
let page
let serverOutput = ''

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>User routes CI</title><div id="root"></div><script type="module" src="/.user-routes-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import './src/index.css';
    import './src/i18n';
    import { UserRoutePage } from './src/features/user-routes/user-route-page';
    import { ApiKeyForm } from './src/features/api-keys/api-key-form';
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function App() {
      const [view, setView] = useState('routes');
      const [issued, setIssued] = useState(null);
      return <HeroUIProvider>
        <QueryClientProvider client={queryClient}>
          <main style={{ padding: 16, maxWidth: 1000, margin: 'auto' }}>
            <nav aria-label="CI fixture"><button onClick={() => setView('routes')}>routes</button><button onClick={() => setView('key')}>key</button></nav>
            {view === 'routes' ? <UserRoutePage /> : <ApiKeyForm onCancel={() => setView('routes')} onIssued={setIssued} onSaved={() => setView('routes')} />}
            {issued ? <output data-testid="issued-key">{issued.token?.key_prefix || issued.token?.id || 'issued'}</output> : null}
          </main>
        </QueryClientProvider>
      </HeroUIProvider>;
    }
    createRoot(document.getElementById('root')).render(<BrowserRouter><App /></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try { if ((await fetch(`${origin}/.user-routes-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    sessionStorage.setItem('anyflows.management.access-token', 'ci-user-routes')
  })
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    requests.push({ method: request.method(), path, body: request.postData() ? request.postDataJSON() : null })
    assert.equal(request.headers().authorization, 'Bearer ci-user-routes')
    if (path === '/api/token-groups' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ groups, next_cursor: null }) })
      return
    }
    if (path === '/api/user-routes' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ items: routes, capacity: 32 }) })
      return
    }
    if (path === '/api/merchant-pool/routes' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ items: [], capacity: 32 }) })
      return
    }
    if (path === '/api/user-routes' && request.method() === 'POST') {
      const body = request.postDataJSON()
      assert.equal(body.members.length, 2)
      assert.equal(body.members[0].group_id, 1)
      assert.equal(body.members[1].group_id, 2)
      assert.equal(body.strategy, 'price_first')
      if (routePostFailures > 0) {
        routePostFailures -= 1
        await route.fulfill({ status: 409, contentType: 'application/json', body: JSON.stringify({ code: 'user_route_limit_reached' }) })
        return
      }
      routes = [{ id: routeId, ...body }]
      await route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify(routes[0]) })
      return
    }
    if (path.startsWith('/api/user-routes/') && request.method() === 'PUT') {
      const id = Number(path.split('/').at(-1))
      const body = request.postDataJSON()
      routes = routes.map(route => route.id === id ? { ...route, ...body } : route)
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify(routes.find(route => route.id === id)) })
      return
    }
    if (path === '/api/models' && request.method() === 'GET') {
      await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ pricing_scope: 'group', next_cursor: null, models: [] }) })
      return
    }
    if (path === '/api/tokens' && request.method() === 'POST') {
      const body = request.postDataJSON()
      assert.equal(body.group_id, null)
      assert.equal(body.user_route_id, routeId)
      if (tokenPostFailures > 0) {
        tokenPostFailures -= 1
        await route.fulfill({ status: 403, contentType: 'application/json', body: JSON.stringify({ code: 'user_route_unavailable' }) })
        return
      }
      issued = { token: { id: 77, key_prefix: 'sk-ci-route', name: body.name } }
      await route.fulfill({ status: 201, contentType: 'application/json', body: JSON.stringify(issued) })
      return
    }
    await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ code: 'not_found' }) })
  })
  await page.goto(`${origin}/.user-routes-smoke.html`)
  // HeroUI includes the selected value before the field label in the accessible name.
  const selectTrigger = name => page.getByRole('button', { name }).or(page.getByRole('combobox', { name })).filter({ visible: true })
  await page.getByRole('heading', { name: label('userRoutes.title'), exact: true }).waitFor()
  await page.getByRole('button', { name: label('userRoutes.actions.create'), exact: true }).click()
  const createDialog = page.getByRole('dialog')
  const name = page.getByRole('textbox', { name: label('userRoutes.fields.name'), exact: true })
  await name.fill('CI route retained')
  await page.getByRole('button', { name: label('userRoutes.actions.addMember'), exact: true }).click()
  await selectTrigger(label('userRoutes.memberGroup').replace('{{number}}', '1')).click()
  const stableOption = label('apiKeys.group.option').replace('{{name}}', groups[1].display_name).replace('{{ratio}}', '1')
  await page.getByRole('option', { name: stableOption, exact: true }).click()
  await page.getByRole('button', { name: label('userRoutes.actions.addMember'), exact: true }).click()
  await createDialog.getByRole('button', { name: label('userRoutes.actions.moveUp'), exact: true }).nth(1).click()
  await selectTrigger(label('userRoutes.memberGroup').replace('{{number}}', '2')).waitFor()
  assert.equal(await page.getByText(label('userRoutes.strategies.price_first'), { exact: true }).count() > 0, true)
  const save = createDialog.getByRole('button', { name: label('userRoutes.actions.create'), exact: true })
  await save.click()
  await page.getByRole('alert').getByText(label('userRoutes.errors.user_route_limit_reached'), { exact: true }).waitFor()
  assert.equal(await name.inputValue(), 'CI route retained')
  await save.click()
  await page.getByRole('heading', { name: label('userRoutes.title'), exact: true }).waitFor()
  await createDialog.waitFor({ state: 'hidden' })
  await page.getByText('CI route retained', { exact: true }).filter({ visible: true }).waitFor()
  assert.equal(routes.length, 1)
  await page.getByRole('button', { name: label('userRoutes.actions.edit'), exact: true }).filter({ visible: true }).click()
  const editName = page.getByRole('textbox', { name: label('userRoutes.fields.name'), exact: true })
  await editName.fill('CI route edited')
  await page.getByRole('button', { name: label('userRoutes.actions.save'), exact: true }).click()
  await page.getByRole('dialog').waitFor({ state: 'hidden' })
  await page.getByText('CI route edited', { exact: true }).filter({ visible: true }).waitFor()
  assert.equal(requests.filter(request => request.method === 'PUT' && request.path === `/api/user-routes/${routeId}`).length, 1)
  await page.getByRole('button', { name: 'key', exact: true }).click()
  await page.getByRole('textbox', { name: label('apiKeys.fields.name'), exact: true }).fill('CI routed key')
  await page.getByRole('button', { name: label('apiKeys.routing.routeMode'), exact: true }).click()
  const routeSelect = page.getByRole('combobox', { name: label('apiKeys.routing.routeLabel'), exact: true })
  await routeSelect.waitFor()
  await routeSelect.selectOption(String(routeId))
  const issue = page.getByRole('button', { name: label('apiKeys.actions.create'), exact: true })
  await issue.click()
  await page.getByText(label('apiKeys.routing.unavailable'), { exact: true }).waitFor()
  assert.equal(await page.getByRole('textbox', { name: label('apiKeys.fields.name'), exact: true }).inputValue(), 'CI routed key')
  await issue.click()
  await page.getByTestId('issued-key').waitFor()
  assert.equal(requests.filter(request => request.method === 'POST' && request.path === '/api/tokens').length, 2)
  assert.deepEqual(requests.filter(request => request.method === 'POST' && request.path === '/api/tokens').at(-1).body, {
    name: 'CI routed key', status: 'enabled', remain_quota: 0, unlimited_quota: true,
    expired_at: null, model_limits: null, allow_ips: null, group_id: null, user_route_id: routeId,
  })
  await page.screenshot({ path: resolve(screenshots, 'user-route-and-key.png'), fullPage: true, animations: 'disabled' })
  assert.deepEqual(errors, [])
  console.log('User route browser checks passed: create failure retains draft, retry creates and edits a route, Key binds an enabled route, and route failure exposes a retryable error.')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  const ariaSnapshot = page ? await page.locator('body').ariaSnapshot().catch(() => undefined) : undefined
  console.error(JSON.stringify({ browserErrors: errors, requests, ariaSnapshot }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}

