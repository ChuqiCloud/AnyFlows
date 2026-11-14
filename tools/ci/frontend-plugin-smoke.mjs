import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Plugin browser checks run only in remote CI')
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
const html = resolve(directory, '.plugin-smoke.html')
const entry = resolve(directory, '.plugin-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'plugin-screenshots-web-next')
const port = 4189
const origin = `http://127.0.0.1:${port}`
const now = Math.floor(Date.now() / 1000)
const digest = 'ab'.repeat(32)
const artifact = {
  id: 1, pluginId: 'org.example.catalog', pluginVersion: '1.0.0', digest,
  manifest: { pluginId: 'org.example.catalog', pluginVersion: '1.0.0', displayName: 'CI model directory', capabilities: [{ id: 'catalog.models.read', version: 1, access: 'read', sideEffect: 'none' }], configSchema: { type: 'object' } },
  sourceKind: 'local_process', signerKeyId: 'ci-official-root', official: true,
  createdAt: now, approval: { approvedBy: 1, approvedAt: now, requestId: 'ci-approved' }, revocation: null,
}
const revoked = { ...artifact, id: 2, pluginId: 'org.example.revoked', digest: 'cd'.repeat(32), signerKeyId: '', official: false, revocation: { revokedBy: 1, revokedAt: now, requestId: 'ci-revoked' } }
let deployment = { pluginId: artifact.pluginId, generation: 4, artifact, enabled: true, grantedCapabilities: [{ id: 'catalog.models.read', version: 1 }], updatedBy: 1, updatedAt: now, requestId: 'ci-deployment' }
let source = { pluginId: artifact.pluginId, pluginVersion: '1.0.0', displayName: 'CI model directory', generation: 4, supportsSearch: true }
const model = { id: 'fixture-text', displayName: 'Fixture text model', provider: 'example', description: 'Plugin reference entry', contextWindow: 32768, inputModalities: ['text'], outputModalities: ['text'], tags: ['fixture'] }
const catalogCalls = [], installs = [], uninstalls = [], cleanups = [], requests = [], errors = []
let modelFailure = false, sourceFailure = false, runtimeFailure = false
let runtimeRequests = 0, deploymentRequests = 0, catalogRequests = 0
let server, browser, page, serverOutput = ''

async function waitUntil(predicate, message) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (predicate()) return
    await delay(100)
  }
  assert.fail(message)
}

function runtime() {
  return {
    localNodeId: 'node-a', isolation: 'auto',
    nodes: [
      { nodeId: 'node-a', hostLabel: 'CI node A', lastSeenAt: now, live: true },
      { nodeId: 'node-b', hostLabel: 'CI node B', lastSeenAt: now, live: true },
      { nodeId: 'node-c', hostLabel: 'CI node C', lastSeenAt: now, live: true },
      { nodeId: 'node-old', hostLabel: 'CI expired node', lastSeenAt: now - 300, live: false },
    ],
    states: [
      { nodeId: 'node-a', pluginId: artifact.pluginId, generation: deployment.generation, digest, status: deployment.enabled ? 'ready' : 'stopped', isolation: 'resource_limited', errorCode: null, updatedAt: now, nodeLastSeenAt: now },
      { nodeId: 'node-b', pluginId: artifact.pluginId, generation: deployment.generation - 1, digest, status: 'ready', isolation: 'process', errorCode: null, updatedAt: now, nodeLastSeenAt: now },
      { nodeId: 'node-c', pluginId: artifact.pluginId, generation: deployment.generation, digest, status: 'failed', isolation: 'unknown', errorCode: 'startup_failed', updatedAt: now, nodeLastSeenAt: now },
      { nodeId: 'node-old', pluginId: artifact.pluginId, generation: deployment.generation, digest, status: 'failed', isolation: 'unknown', errorCode: 'startup_failed', updatedAt: now - 300, nodeLastSeenAt: now - 300 },
    ],
  }
}

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.plugin-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import './src/index.css'; import './src/i18n';
    import { useSiteTheme } from './src/shared/components/use-site-theme';
    import { PluginLifecyclePage } from './src/features/plugin-lifecycle/plugin-lifecycle-page';
    import { ModelPage } from './src/features/models/model-page';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    function App() {
      useSiteTheme();
      const [view, setView] = useState('plugins');
      return <HeroUIProvider><QueryClientProvider client={client}><main style={{ padding: 16, maxWidth: 1200, margin: 'auto' }}><nav aria-label="CI views">{['plugins','models','guest'].map(value => <button key={value} onClick={() => setView(value)}>{value}</button>)}</nav>{view === 'plugins' ? <PluginLifecyclePage /> : <ModelPage key={view} authenticated={view === 'models'} standalone />}</main></QueryClientProvider></HeroUIProvider>;
    }
    createRoot(document.getElementById('root')).render(<BrowserRouter><App /></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) { try { if ((await fetch(`${origin}/.plugin-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 1000 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    localStorage.setItem('anyflows.theme', 'light')
    sessionStorage.setItem('anyflows.management.access-token', 'ci-plugin')
  })
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request(), url = new URL(request.url()), path = url.pathname, method = request.method()
    requests.push({ path, method })
    const respond = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    if (path === '/api/site') return respond({ balance_display: { mode: 'quota', unit_name: 'quota', unit_symbol: 'q', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } })
    if (path === '/api/model-provider-catalog') return respond({ providers: [] })
    if (path === '/api/models') return respond({ models: [], next_cursor: null, pricing_scope: 'group' })
    if (path === '/api/model-providers') return respond({ providers: [] })
    assert.equal(request.headers().authorization, 'Bearer ci-plugin', `${method} ${path} must use the current session`)
    if (path === '/api/admin/plugins/artifacts' && method === 'GET') return respond({ artifacts: [artifact, revoked], nextBeforeId: null })
    if (path === '/api/admin/plugins/deployments' && method === 'GET') { deploymentRequests += 1; return respond({ deployments: [deployment], nextCursor: null }) }
    if (path === '/api/admin/plugins/runtime' && method === 'GET') {
      runtimeRequests += 1
      return runtimeFailure ? respond({ error: { code: 'internal' } }, 500) : respond(runtime())
    }
    if (path === '/api/admin/plugins/install' && method === 'POST') {
      assert.match(request.headers()['content-type'], /^multipart\/form-data; boundary=/)
      const body = request.postData() ?? ''
      assert.ok(body.includes('filename="catalog.afplugin"'))
      assert.ok(body.includes('name="config"'))
      assert.equal(body.includes('signer_key_id'), false, 'Normal install has no forced signature input')
      installs.push(body)
      deployment = { ...deployment, generation: deployment.generation + 1 }
      source = { ...source, generation: deployment.generation }
      return respond(deployment)
    }
    if (path === '/api/admin/plugins/org.example.catalog/uninstall' && method === 'POST') {
      const body = request.postDataJSON()
      assert.equal(body.expected_generation, deployment.generation)
      uninstalls.push(body)
      deployment = { ...deployment, generation: deployment.generation + 1, enabled: false }
      return respond({ pluginId: artifact.pluginId, generation: deployment.generation, pendingNodes: 1 })
    }
    if (path === '/api/admin/plugins/artifacts/2/cleanup' && method === 'POST') {
      cleanups.push(2)
      return respond({ artifactId: 2, archiveRemoved: true, localFilesRemoved: true })
    }
    if (path === '/api/account/plugin-models/sources' && method === 'GET') {
      catalogRequests += 1
      return sourceFailure ? respond({ error: { code: 'internal' } }, 500) : respond({ sources: [source], nextCursor: null })
    }
    if (path === '/api/account/plugin-models/org.example.catalog' && method === 'GET') {
      catalogCalls.push(Object.fromEntries(url.searchParams))
      if (modelFailure) return respond({ error: { code: 'plugin_lifecycle_unavailable' } }, 503)
      if (url.searchParams.get('q')) return respond({ source, models: [{ ...model, id: 'fixture-vision', displayName: 'Fixture vision search' }], nextCursor: null })
      if (url.searchParams.get('cursor')) return respond({ source, models: [{ ...model, id: 'fixture-second', displayName: 'Fixture second page' }], nextCursor: null })
      return respond({ source, models: [model], nextCursor: 'opaque-generation-bound-cursor' })
    }
    errors.push(`Unexpected request: ${method} ${path}`)
    return respond({ error: { code: 'not_found' } }, 404)
  })

  await page.goto(`${origin}/.plugin-smoke.html`)
  await page.getByRole('heading', { name: label('pluginLifecycle.title'), exact: true }).waitFor()
  const runtimePanel = page.getByRole('region', { name: label('pluginLifecycle.runtime.title') })
  await runtimePanel.getByText('CI node A', { exact: true }).first().waitFor()
  await runtimePanel.getByText(label('pluginLifecycle.runtime.readyNodes').replace('{{ready}}', '1').replace('{{total}}', '3'), { exact: true }).waitFor()
  await runtimePanel.getByText(label('pluginLifecycle.runtime.states.pending'), { exact: true }).waitFor()
  await runtimePanel.getByText(label('pluginLifecycle.runtime.stale'), { exact: true }).waitFor()
  await runtimePanel.getByText(`${label('pluginLifecycle.runtime.states.failed')} · ${label('pluginLifecycle.runtime.isolationModes.unknown')} · startup_failed`, { exact: true }).waitFor()
  assert.equal(await runtimePanel.getByText(label('pluginLifecycle.runtime.readyNodes').replace('{{ready}}', '3').replace('{{total}}', '3'), { exact: true }).count(), 0, 'An old generation or failed process cannot count as ready')
  const deploymentCard = page.locator('article').filter({ has: page.getByRole('button', { name: label('pluginLifecycle.removal.uninstall'), exact: true }) })
  await deploymentCard.getByText(label('pluginLifecycle.status.official'), { exact: true }).waitFor()
  const advanced = page.locator('details').filter({ has: page.locator('summary').filter({ hasText: label('pluginLifecycle.advanced') }) })
  assert.equal(await advanced.getAttribute('open'), null, 'Normal installation keeps manual fields folded')

  await page.locator('input[type="file"]').setInputFiles({ name: 'invalid.txt', mimeType: 'text/plain', buffer: Buffer.from('not a plugin') })
  await page.getByRole('button', { name: label('pluginLifecycle.install.submit'), exact: true }).click()
  await page.getByText(label('pluginLifecycle.install.errors.packageType'), { exact: true }).waitFor()
  assert.equal(installs.length, 0, 'Invalid package names fail before any upload')
  await page.locator('input[type="file"]').setInputFiles({ name: 'catalog.afplugin', mimeType: 'application/zip', buffer: Buffer.from('mock ZIP bytes; intercepted by CI') })
  const installed = page.waitForResponse(response => new URL(response.url()).pathname === '/api/admin/plugins/install' && response.request().method() === 'POST')
  const previousDeploymentRequests = deploymentRequests, previousRuntimeRequests = runtimeRequests
  await page.getByRole('button', { name: label('pluginLifecycle.install.submit'), exact: true }).click()
  await installed
  await page.getByText(label('pluginLifecycle.install.success').replace('{{plugin}}', artifact.pluginId).replace('{{version}}', artifact.pluginVersion), { exact: true }).waitFor()
  await page.waitForFunction(() => document.querySelector('input[type="file"]')?.files.length === 0)
  assert.equal(installs.length, 1)
  await waitUntil(() => deploymentRequests > previousDeploymentRequests && runtimeRequests > previousRuntimeRequests, 'Install must refresh deployments and runtime observations')
  assert.ok(deploymentRequests > previousDeploymentRequests, 'Install refreshes desired deployment state')
  assert.ok(runtimeRequests > previousRuntimeRequests, 'Install refreshes actual node state')

  await page.getByRole('button', { name: label('pluginLifecycle.removal.uninstall'), exact: true }).click()
  const uninstallDialog = page.getByRole('dialog')
  await uninstallDialog.getByText(label('pluginLifecycle.removal.uninstallHint'), { exact: true }).waitFor()
  await uninstallDialog.getByRole('button', { name: label('pluginLifecycle.confirm'), exact: true }).click()
  await uninstallDialog.waitFor({ state: 'hidden' })
  await page.getByText(label('pluginLifecycle.removal.accepted').replace('{{count}}', '1'), { exact: true }).waitFor()
  assert.equal(uninstalls.length, 1)

  await advanced.locator('summary').first().click()
  await page.getByRole('button', { name: label('pluginLifecycle.removal.cleanup'), exact: true }).click()
  const cleanupDialog = page.getByRole('dialog')
  await cleanupDialog.getByText(label('pluginLifecycle.removal.cleanupHint'), { exact: true }).waitFor()
  await cleanupDialog.getByRole('button', { name: label('pluginLifecycle.confirm'), exact: true }).click()
  await cleanupDialog.waitFor({ state: 'hidden' })
  await page.getByText(label('pluginLifecycle.removal.cleaned'), { exact: true }).waitFor()
  assert.deepEqual(cleanups, [2])
  await page.screenshot({ path: resolve(screenshots, 'plugin-management-desktop.png'), fullPage: true })

  runtimeFailure = true
  await runtimePanel.getByRole('button', { name: label('pluginLifecycle.refresh'), exact: true }).click()
  await runtimePanel.getByText(label('pluginLifecycle.runtime.error'), { exact: true }).waitFor()
  runtimeFailure = false
  await runtimePanel.getByRole('button', { name: label('pluginLifecycle.refresh'), exact: true }).click()
  await runtimePanel.getByText('CI node A', { exact: true }).first().waitFor()

  await page.getByRole('navigation', { name: 'CI views' }).getByRole('button', { name: 'models', exact: true }).click()
  await page.getByRole('heading', { name: label('pluginCatalog.title'), exact: true }).waitFor()
  const catalog = page.locator('section').filter({ has: page.getByRole('heading', { name: label('pluginCatalog.title'), exact: true }) })
  await catalog.locator('select').selectOption(artifact.pluginId)
  await catalog.getByRole('heading', { name: 'Fixture text model', exact: true }).waitFor()
  await catalog.getByRole('button', { name: label('pluginCatalog.moreModels'), exact: true }).click()
  await catalog.getByRole('heading', { name: 'Fixture second page', exact: true }).waitFor()
  assert.ok(catalogCalls.some(call => call.cursor === 'opaque-generation-bound-cursor'))
  const beforeRefresh = catalogCalls.length
  await catalog.getByRole('button', { name: label('pluginCatalog.refresh'), exact: true }).click()
  await waitUntil(() => catalogCalls.length >= beforeRefresh + 2, 'Refreshing a paginated catalog reloads both pages')
  await catalog.getByRole('heading', { name: 'Fixture text model', exact: true }).waitFor()
  await catalog.getByRole('heading', { name: 'Fixture second page', exact: true }).waitFor()
  await catalog.getByLabel(label('pluginCatalog.search'), { exact: true }).fill('vision')
  await catalog.getByRole('heading', { name: 'Fixture vision search', exact: true }).waitFor()
  assert.ok(catalogCalls.some(call => call.q === 'vision' && !call.cursor), 'A new search discards prior pagination')

  modelFailure = true
  await catalog.getByRole('button', { name: label('pluginCatalog.refresh'), exact: true }).click()
  await catalog.getByRole('alert').getByText(label('pluginCatalog.modelsError'), { exact: true }).first().waitFor()
  modelFailure = false
  await catalog.getByRole('button', { name: label('pluginCatalog.refresh'), exact: true }).click()
  await catalog.getByRole('heading', { name: 'Fixture vision search', exact: true }).waitFor()
  await page.setViewportSize({ width: 390, height: 844 })
  await page.screenshot({ path: resolve(screenshots, 'plugin-catalog-mobile.png'), fullPage: true })
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)
  assert.ok(overflow <= 1, `Plugin model catalog overflowed a mobile viewport by ${overflow}px`)

  const previousCatalogRequests = catalogRequests
  await page.getByRole('navigation', { name: 'CI views' }).getByRole('button', { name: 'guest', exact: true }).click()
  await page.getByRole('heading', { name: label('models.title'), exact: true }).waitFor()
  assert.equal(await page.getByRole('heading', { name: label('pluginCatalog.title'), exact: true }).count(), 0, 'Guest model page hides the authenticated plugin catalog')
  await delay(750)
  assert.equal(catalogRequests, previousCatalogRequests, 'Guest view never fetches private plugin catalog sources')

  sourceFailure = true
  await page.reload()
  await page.getByRole('navigation', { name: 'CI views' }).getByRole('button', { name: 'models', exact: true }).click()
  await page.getByText(label('pluginCatalog.sourceError'), { exact: true }).waitFor()
  sourceFailure = false
  await page.getByRole('button', { name: label('pluginCatalog.retry'), exact: true }).click()
  await page.locator('section').filter({ has: page.getByRole('heading', { name: label('pluginCatalog.title'), exact: true }) }).locator('select').selectOption(artifact.pluginId)
  await page.getByRole('heading', { name: 'Fixture text model', exact: true }).waitFor()

  assert.deepEqual(errors, [])
  console.log('Plugin lifecycle and user catalog browser regressions passed; all API calls were mocked')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true }).catch(() => {})
  console.error(JSON.stringify({ error: String(error), errors, requests, catalogCalls, installs: installs.length, uninstalls, cleanups, serverOutput }, null, 2))
  throw error
} finally {
  await browser?.close()
  server?.kill('SIGTERM')
  await Promise.all([html, entry].map(path => unlink(path).catch(() => {})))
}
