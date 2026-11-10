import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Key capability checks run in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT)
assert.ok(process.env.RUNNER_TEMP)
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const translations = JSON.parse(await readFile(resolve(directory, 'src/i18n/key-capabilities.json'), 'utf8'))
const label = (key, vars = {}) => Object.entries(vars).reduce((value, [name, item]) => value.replaceAll(`{{${name}}}`, String(item)), key.split('.').reduce((node, part) => node[part], translations.zh))
const html = resolve(directory, '.key-capabilities-smoke.html')
const entry = resolve(directory, '.key-capabilities-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'key-capabilities-screenshots-web-next')
const port = 4184
const origin = `http://127.0.0.1:${port}`
const policy = () => ({ policy: { mode: 'inherit', models: [], disabled_models: [] }, version: 0, auto_add_discovered: false, refresh_interval_secs: null })
const policies = new Map([['channel', policy()], ['11', policy()], ['12', policy()], ['13', policy()]])
policies.get('channel').policy = { mode: 'denylist', models: ['Raw-B'], disabled_models: [] }
const explicitPolicies = new Set(['channel'])
function modelSettings(id) {
  const own = policies.get(id)
  if (explicitPolicies.has(id)) return own
  const parent = policies.get('channel')
  return { ...own, version: 0, auto_add_discovered: parent.auto_add_discovered, refresh_interval_secs: parent.refresh_interval_secs }
}
function allows(rule, name) { return !rule.disabled_models.includes(name) && (rule.mode === 'inherit' || (rule.mode === 'allowlist' ? rule.models.includes(name) : !rule.models.includes(name))) }
const items = [
  { upstream_model: 'Raw-A', canonical_models: ['public-a'], enabled: true, blocked_reason: null, source: 'discovered' },
  { upstream_model: 'Raw-B', canonical_models: ['public-b'], enabled: false, blocked_reason: 'channel_policy', source: 'discovered' },
]
const observations = new Map(['11', '12', '13'].map((id) => [id, { items, status: 'available', last_success_at: 1700000000, last_checked_at: 1700000000, error_code: null }]))
function modelSnapshot(id) {
  const settings = modelSettings(id)
  const records = new Map(observations.get(id).items.map(item => [item.upstream_model, { ...item }]))
  for (const name of [...settings.policy.models, ...settings.policy.disabled_models]) if (!records.has(name)) records.set(name, { upstream_model: name, canonical_models: [], source: 'manual' })
  const effective = [...records.values()].map(item => {
    const reason = id === '12' ? 'credential_disabled' : item.canonical_models.length === 0 ? 'channel_not_published' : !allows(policies.get('channel').policy, item.upstream_model) ? 'channel_policy' : !allows(settings.policy, item.upstream_model) ? 'credential_policy' : null
    return { ...item, blocked_reason: reason, enabled: reason === null }
  })
  return { ...observations.get(id), ...settings, items: effective }
}
const usageDefault = () => ({ version: 0, provider: 'auto', scope: 'key', profile_id: null, refresh_interval_secs: null, quota_per_unit: null, currency: null, low_balance_threshold: null, pause_when_exhausted: false, inherited: true })
const configs = new Map([['11', usageDefault()], ['13', usageDefault()], ['channel', usageDefault()]])
const usage = { status: 'available', scope: 'account', provider: 'new_api', windows: [], credits_balance: null, fetched_at: 1700000000, currency: 'USD', unit: 'USD', remaining: '0', used: '10', total: null, unlimited: false, last_success_at: 1700000000, last_checked_at: 1700000000, stale: false, error_code: null, account_ref: 'shared-21' }
const modelFetches = []
const policyWrites = []
const configWrites = []
const profileWrites = []
const browserErrors = []
let profiles = []
let policyFailures = 1
let configFailures = 1
let profileFailures = 1
let model11Failures = 1
let model13Failures = 1
let usageRefreshes = 0
let coverageFailures = 1
let catalogPages = 0
let server
let browser
let page
let serverOutput = ''

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.key-capabilities-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    import { CredentialList } from './src/features/credentials/credential-list';
    import { UpstreamModelPolicyEditor } from './src/features/credentials/upstream-model-policy-editor';
    import { ChannelModelCoverage } from './src/features/credentials/channel-model-coverage';
    import './src/index.css'; import './src/i18n';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } } });
    const credentials = [11, 12, 13].map(id => ({ id, channel_id: 7, kind: 'api_key', status: id === 12 ? 'disabled' : 'enabled', schedulable: true, priority: 0, weight: 10, concurrency: null, oauth_token_pending: false, parent_id: null, quota_dimension: 'global', last_used_at: null, rate_limited_at: null, rate_limit_reset_at: null, overload_until: null, temp_unschedulable_until: null }));
    function Fixture() { const [view, setView] = useState('keys'); return <HeroUIProvider><QueryClientProvider client={client}><main className="mx-auto max-w-5xl p-4"><nav><button onClick={() => setView('keys')}>keys</button><button onClick={() => setView('channel')}>channel</button></nav>{view === 'keys' ? <CredentialList channel={{id:7,type:'openai',protocol:'openai_chat',provider:'openai',name:'Fixture'}} credentials={credentials} loading={false} loadError={false} refreshing={false} refreshError={false} loadingMore={false} hasNextPage={false} active onRefresh={async () => {}} onLoadMore={async () => {}} onEdit={() => {}} /> : <><UpstreamModelPolicyEditor channelId={7} /><ChannelModelCoverage channelId={7} models={['public-a','public-b','public-c']} /></>}</main></QueryClientProvider></HeroUIProvider> }
    createRoot(document.getElementById('root')).render(<Fixture />);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) { try { if ((await fetch(`${origin}/.key-capabilities-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => browserErrors.push(error.stack || error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); sessionStorage.setItem('anyflows.management.access-token', 'ci-key-capabilities') })
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    const method = request.method()
    assert.equal(request.headers().authorization, 'Bearer ci-key-capabilities')
    const respond = (data, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) })
    if (path === '/api/admin/channels/7/credentials') {
      const after = new URL(request.url()).searchParams.get('after')
      catalogPages += 1
      return respond({ credentials: (after ? [13] : [11, 12]).map(id => ({ id, channel_id: 7, kind: 'api_key', status: id === 12 ? 'disabled' : 'enabled', schedulable: true, priority: 0, weight: 10, concurrency: null, parent_id: null, quota_dimension: 'global', last_used_at: null, oauth_token_pending: false })), next_cursor: after ? null : 12 })
    }
    if (path.endsWith('/credentials/health')) return respond({ available: true, credentials: [] })
    const target = path.match(/\/credentials\/(\d+)\//)?.[1] ?? 'channel'
    if (path.endsWith('/model-policy')) {
      if (method === 'GET') return respond(modelSettings(target))
      const body = request.postDataJSON(); policyWrites.push({ target, body })
      if (target === '11' && policyFailures-- > 0) return respond({ code: 'internal_error' }, 500)
      assert.equal(body.version, modelSettings(target).version)
      policies.set(target, { ...body, version: body.version + 1 })
      explicitPolicies.add(target)
      return respond(policies.get(target))
    }
    if (path.endsWith('/models/refresh')) {
      modelFetches.push(target)
      const failed = target === '11' ? model11Failures-- > 0 : target === '13' ? model13Failures-- > 0 : false
      const observation = { ...observations.get(target), status: failed ? 'unavailable' : 'available', error_code: failed ? 'query_failed' : null }
      observations.set(target, observation)
      return respond(modelSnapshot(target))
    }
    if (path.endsWith('/models')) {
      if (target === '13' && coverageFailures-- > 0) return respond({ code: 'internal_error' }, 503)
      return respond(modelSnapshot(target))
    }
    if (path.endsWith('/usage-config')) {
      if (method === 'GET') return respond(configs.get(target))
      if (method === 'DELETE') { configs.set(target, usageDefault()); return route.fulfill({ status: 204 }) }
      const body = request.postDataJSON(); configWrites.push({ target, body })
      if (target === '11' && configFailures-- > 0) return respond({ code: 'internal_error' }, 500)
      configs.set(target, { ...body, inherited: false, version: body.version + 1 })
      return respond(configs.get(target))
    }
    if (path.endsWith('/usage/refresh')) { usageRefreshes += 1; return respond({ ...usage, status: usageRefreshes === 1 ? 'unavailable' : 'available', stale: usageRefreshes === 1, error_code: usageRefreshes === 1 ? 'upstream_rate_limited' : null }) }
    if (path.endsWith('/usage')) {
      if (target === '13') return respond({ ...usage, scope: 'window', provider: 'codex', currency: null, remaining: null, used: null, windows: [{ window_seconds: 18000, used_percent: 25, reset_at: 1700010000 }], credits_balance: '7.5' })
      if (configs.get(target)?.provider === 'none') return respond({ ...usage, scope: 'key', status: 'unsupported', currency: null, remaining: null, used: null, total: null })
      return respond(usage)
    }
    if (path === '/api/admin/upstream-usage-profiles') {
      if (method === 'GET') return respond({ items: profiles })
      const body = request.postDataJSON(); profileWrites.push({ ...body, secret: body.secret ? '<present>' : null })
      if (profileFailures-- > 0) return respond({ code: 'internal_error' }, 500)
      profiles = [{ ...body, id: 21, version: 1, has_secret: true }].map(({ secret: _secret, ...profile }) => profile)
      return respond(profiles[0], 201)
    }
    if (path === '/api/admin/upstream-usage-profiles/21') {
      if (method === 'DELETE') return respond({ code: 'profile_in_use' }, 409)
      const body = request.postDataJSON(); profileWrites.push({ ...body, secret: body.secret ? '<present>' : null })
      profiles = [{ ...profiles[0], ...body, version: body.version + 1 }].map(({ secret: _secret, ...profile }) => profile)
      return respond(profiles[0])
    }
    throw new Error(`Unexpected ${method} ${path}`)
  })
  await page.goto(`${origin}/.key-capabilities-smoke.html`)
  const keyRow = id => page.locator('article').filter({ has: page.getByRole('heading', { name: `#${id}`, exact: true }) })
  const dialog = () => page.getByRole('dialog').last()
  const saveModelPolicy = async () => {
    const done = page.waitForResponse(response => response.url().endsWith('/model-policy') && response.request().method() === 'PUT' && response.status() === 200)
    await dialog().getByRole('button', { name: label('savePolicy'), exact: true }).click()
    await done
    await dialog().getByRole('button', { name: label('savePolicy'), exact: true }).waitFor({ state: 'visible' })
    await page.waitForFunction(text => [...document.querySelectorAll('button')].some(button => button.textContent?.includes(text) && button.disabled), label('savePolicy'))
  }
  await keyRow(11).getByRole('button', { name: label('fetchModels'), exact: true }).click()
  await dialog().getByText(label('discoveryFailed'), { exact: true }).waitFor()
  assert.deepEqual(modelFetches, ['11'], 'Single Key discovery never substitutes a sibling')
  await dialog().getByRole('button', { name: label('fetchModels'), exact: true }).click()
  await dialog().getByText(label('discoveryFailed'), { exact: true }).waitFor({ state: 'hidden' })
  await dialog().getByText(label('reason.channel_policy'), { exact: true }).waitFor()
  assert.equal(await dialog().getByRole('switch', { name: label('toggleModel', { model: 'Raw-B' }), exact: true }).isChecked(), true, 'Channel denial is not stored as a local disable')
  await dialog().getByRole('combobox', { name: label('policyMode'), exact: true }).selectOption('allowlist')
  await dialog().getByRole('textbox', { name: label('exactNames'), exact: true }).fill('')
  await dialog().getByRole('button', { name: label('savePolicy'), exact: true }).click()
  await dialog().getByRole('alert').filter({ hasText: label('error') }).waitFor()
  assert.equal(await dialog().getByRole('combobox', { name: label('policyMode'), exact: true }).inputValue(), 'allowlist')
  await saveModelPolicy()
  await dialog().getByText(label('saved'), { exact: true }).waitFor()
  assert.deepEqual(policyWrites.at(-1).body.policy, { mode: 'allowlist', models: [], disabled_models: [] }, 'Empty allowlist remains an explicit denial')
  await dialog().getByRole('row').filter({ hasText: 'Raw-A' }).getByText(label('reason.credential_policy'), { exact: true }).waitFor()
  await dialog().getByRole('textbox', { name: label('manualModel'), exact: true }).fill('Case-Sensitive')
  await dialog().getByRole('button', { name: label('add'), exact: true }).click()
  await dialog().getByRole('textbox', { name: label('searchModels'), exact: true }).fill('Case-Sensitive')
  await dialog().getByRole('checkbox', { name: label('selectModels'), exact: true }).check()
  await dialog().getByRole('button', { name: label('disableSelected'), exact: true }).click()
  await saveModelPolicy()
  await dialog().getByText(label('saved'), { exact: true }).waitFor()
  assert.ok(policyWrites.at(-1).body.policy.models.includes('Case-Sensitive'))
  assert.ok(policyWrites.at(-1).body.policy.disabled_models.includes('Case-Sensitive'))
  await dialog().getByRole('button', { name: label('close'), exact: true }).click(); await dialog().waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('fetchEnabled', { count: 2 }), exact: true }).click()
  await page.getByText(label('bulkProgress', { done: 2, total: 2 }), { exact: true }).waitFor()
  assert.deepEqual(modelFetches.slice(-2), ['11', '13'], 'Bulk enabled discovery excludes disabled Key 12')
  await page.getByRole('button', { name: label('retryFailed', { count: 1 }), exact: true }).click()
  await page.getByText(label('bulkProgress', { done: 1, total: 1 }), { exact: true }).waitFor()
  assert.equal(modelFetches.at(-1), '13')

  await keyRow(11).getByRole('button', { name: label('upstreamUsage'), exact: true }).click()
  await dialog().locator('strong').getByText(label('scope.account'), { exact: true }).waitFor()
  await dialog().getByText('0 USD', { exact: true }).waitFor()
  await dialog().getByRole('combobox', { name: label('queryScope'), exact: true }).selectOption('account')
  await dialog().getByRole('button', { name: label('createProfile'), exact: true }).click()
  await dialog().getByRole('textbox', { name: label('profileName'), exact: true }).fill('CI shared account')
  await dialog().getByRole('textbox', { name: label('baseUrl'), exact: true }).fill('https://upstream.example')
  await dialog().getByLabel(label('managementSecret'), { exact: true }).fill('ci-read-only-secret')
  await dialog().getByRole('button', { name: label('saveProfile'), exact: true }).click()
  await dialog().getByRole('alert').waitFor()
  assert.equal(await dialog().getByLabel(label('managementSecret'), { exact: true }).inputValue(), 'ci-read-only-secret')
  await dialog().getByRole('button', { name: label('saveProfile'), exact: true }).click()
  await page.getByRole('dialog').filter({ has: page.getByRole('textbox', { name: label('profileName'), exact: true }) }).waitFor({ state: 'hidden' })
  await dialog().getByRole('combobox', { name: label('sharedProfile'), exact: true }).selectOption('21')
  assert.equal(await dialog().getByRole('switch').count(), 2, 'Usage configuration exposes auto-refresh and exhaustion controls')
  await dialog().getByRole('switch').first().click()
  await dialog().getByRole('textbox', { name: label('lowThreshold'), exact: true }).fill('0')
  await dialog().getByRole('button', { name: label('saveUsage'), exact: true }).click()
  await dialog().getByRole('alert').filter({ hasText: label('error') }).waitFor()
  assert.equal(await dialog().getByRole('combobox', { name: label('sharedProfile'), exact: true }).inputValue(), '21')
  await dialog().getByRole('button', { name: label('saveUsage'), exact: true }).click()
  await dialog().getByText(label('saved'), { exact: true }).waitFor()
  assert.equal(configWrites.at(-1).body.profile_id, 21)
  assert.equal(configWrites.at(-1).body.provider, 'new_api')
  assert.equal(configWrites.at(-1).body.refresh_interval_secs, 900)
  assert.equal(configWrites.at(-1).body.low_balance_threshold, '0')
  await dialog().getByRole('button', { name: label('editProfile'), exact: true }).click()
  assert.equal(await dialog().getByLabel(label('managementSecret'), { exact: true }).inputValue(), '')
  await dialog().getByRole('button', { name: label('deleteProfile'), exact: true }).click()
  await dialog().getByRole('button', { name: label('confirmDeleteProfile'), exact: true }).click()
  await dialog().getByText(label('profileInUse'), { exact: true }).waitFor()
  await dialog().getByRole('button', { name: label('saveProfile'), exact: true }).click()
  await page.getByRole('dialog').filter({ has: page.getByRole('textbox', { name: label('profileName'), exact: true }) }).waitFor({ state: 'hidden' })
  assert.equal(profileWrites.at(-1).secret, null, 'Editing preserves a write-only secret when blank')
  await dialog().getByRole('button', { name: label('refreshUsage'), exact: true }).click()
  await dialog().getByText(label('stale'), { exact: true }).waitFor()
  await dialog().getByText('0 USD', { exact: true }).waitFor()
  await dialog().getByText(label('usageError.upstream_rate_limited'), { exact: true }).waitFor()
  await dialog().getByRole('button', { name: label('refreshUsage'), exact: true }).click()
  await dialog().getByText(label('stale'), { exact: true }).waitFor({ state: 'hidden' })
  await dialog().getByRole('button', { name: label('restoreUsageInheritance'), exact: true }).click()
  await dialog().getByText(label('usageInherited'), { exact: true }).waitFor()
  await page.setViewportSize({ width: 390, height: 844 })
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), 'Quota drawer fits a mobile viewport')
  await page.screenshot({ path: resolve(screenshots, 'quota-mobile.png'), fullPage: true })
  await dialog().getByRole('button', { name: label('close'), exact: true }).click(); await dialog().waitFor({ state: 'hidden' })
  await keyRow(13).getByRole('button', { name: label('upstreamUsage'), exact: true }).click()
  await dialog().locator('strong').getByText(label('scope.window'), { exact: true }).waitFor()
  await dialog().getByText('25.0%', { exact: false }).waitFor()
  await dialog().getByText('7.5', { exact: true }).waitFor()
  await dialog().getByRole('button', { name: label('close'), exact: true }).click(); await dialog().waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: label('channelUsageDefaults'), exact: true }).click()
  await dialog().getByText(label('channelUsageHint'), { exact: true }).waitFor()
  await dialog().getByRole('combobox', { name: label('provider'), exact: true }).selectOption('none')
  await dialog().getByRole('button', { name: label('saveUsage'), exact: true }).click()
  await dialog().getByText(label('saved'), { exact: true }).waitFor()
  assert.equal(configWrites.at(-1).target, 'channel')
  assert.equal(configWrites.at(-1).body.refresh_interval_secs, null, 'Background polling stays off by default')
  await dialog().getByRole('button', { name: label('close'), exact: true }).click(); await dialog().waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: 'channel', exact: true }).click()
  await page.getByRole('combobox', { name: label('policyMode'), exact: true }).selectOption('denylist')
  await page.getByRole('textbox', { name: label('exactNames'), exact: true }).fill('Raw-B')
  await page.getByRole('region', { name: label('channelPolicy'), exact: true }).getByRole('switch').nth(1).click()
  await page.getByRole('button', { name: label('savePolicy'), exact: true }).click()
  await page.getByText(label('saved'), { exact: true }).waitFor()
  assert.equal(policyWrites.at(-1).target, 'channel')
  assert.deepEqual(policyWrites.at(-1).body.policy.models, ['Raw-B'])
  assert.equal(policyWrites.at(-1).body.refresh_interval_secs, 21600)
  const fetchCountBeforeCoverage = modelFetches.length
  const pagesBeforeCoverage = catalogPages
  await page.getByRole('button', { name: label('readCoverage'), exact: true }).click()
  const coverage = page.getByRole('region', { name: label('coverageTitle'), exact: true })
  await coverage.getByText(label('partialCoverage', { count: 1 }), { exact: true }).waitFor()
  await coverage.getByRole('row').filter({ hasText: 'public-a' }).getByText(label('unknown'), { exact: true }).waitFor()
  assert.ok(catalogPages >= pagesBeforeCoverage + 2, 'Coverage reads every credential page')
  await coverage.getByRole('button', { name: label('refreshCoverage'), exact: true }).click()
  await coverage.getByText(label('modelUnion', { count: 1, total: 3 }), { exact: true }).waitFor()
  await coverage.getByRole('row').filter({ hasText: 'public-a' }).getByText(label('keyCount', { count: 1 }), { exact: true }).waitFor()
  await coverage.getByRole('row').filter({ hasText: 'public-b' }).getByText(label('keyCount', { count: 0 }), { exact: true }).waitFor()
  assert.equal(modelFetches.length, fetchCountBeforeCoverage, 'Coverage uses database GETs without querying upstreams')
  await page.getByRole('button', { name: 'keys', exact: true }).click()
  await keyRow(12).getByRole('button', { name: label('manageModels'), exact: true }).click()
  await dialog().getByRole('textbox', { name: label('modelInterval'), exact: true }).waitFor()
  assert.equal(await dialog().getByRole('textbox', { name: label('modelInterval'), exact: true }).inputValue(), '21600', 'Unconfigured Key inherits the channel refresh interval')
  await dialog().getByRole('region', { name: label('keyPolicy'), exact: true }).getByRole('switch').nth(1).click()
  await saveModelPolicy()
  assert.equal(policyWrites.at(-1).target, '12')
  assert.equal(policyWrites.at(-1).body.refresh_interval_secs, null, 'Explicit off overrides the inherited refresh interval')
  assert.equal(policyWrites.at(-1).body.version, 0)
  assert.deepEqual(browserErrors, [])
  console.log('Key capability browser checks passed: exact Key discovery, retained failure state, empty allowlist, manual models, channel limits, bounded bulk results, shared quota profiles, write-only secrets and stale official/account snapshots.')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors, modelFetches, policyWrites, configWrites, profileWrites }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
