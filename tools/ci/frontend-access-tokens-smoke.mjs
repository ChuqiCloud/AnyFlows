import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'Access token browser checks run in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Use remote Playwright')
assert.ok(process.env.RUNNER_TEMP, 'Save screenshots on the remote runner')
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const copy = JSON.parse(await readFile(resolve(directory, 'src/i18n/access-tokens.json'), 'utf8')).zh
const commonCopy = JSON.parse(await readFile(resolve(directory, 'src/i18n/locales/zh.json'), 'utf8'))
const label = (key, vars = {}) => Object.entries(vars).reduce((value, [name, item]) => value.replaceAll(`{{${name}}}`, String(item)), key.split('.').reduce((node, part) => node?.[part], copy))
const commonLabel = key => key.split('.').reduce((node, part) => node?.[part], commonCopy)
const html = resolve(directory, '.access-token-smoke.html')
const entry = resolve(directory, '.access-token-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, 'access-tokens-screenshots-web-next')
const port = 4187
const origin = `http://127.0.0.1:${port}`
const secret = `af_pat_${'A'.repeat(43)}`
const requests = []
const errors = []
const adminScopes = [
  { scope: 'account:read', label: '账户资料', description: '仅当前账户', admin_only: false },
  { scope: 'api_keys:read', label: '模型 Key 读取', description: '读取本人 Key', admin_only: false },
  { scope: 'api_keys:write', label: '模型 Key 写入', description: '管理本人 Key', admin_only: false },
  { scope: 'admin:channel:write', label: '渠道管理 · 更新', description: '更新渠道', admin_only: true },
  { scope: 'admin:user:write', label: '用户管理 · 更新', description: '管理其他用户', admin_only: true },
]
const userScopes = adminScopes.filter(scope => !scope.admin_only)
let role = 'admin'
let issuedTokensByRole = { admin: [], user: [] }
let policy = { enabled: true, issuance_enabled: true, max_tokens_per_user: 20, version: 1 }
let rejectFirstProof = true
let createCalls = 0
let updateCalls = 0
let revokeCalls = 0
let patExplorerAuthorization
let modelKeyHeader
let modelKeyAuthorization
let gatewayRequestCount = 0
let server, browser, page, userContext
let serverOutput = ''

const preset = scopes => [
  { id: 'account_read', label: '账户只读', scopes: ['account:read'] },
  { id: 'api_keys', label: '模型 Key 管理', scopes: ['api_keys:read', 'api_keys:write'] },
  ...(role === 'admin' ? [{ id: 'admin_full', label: '管理员全权限', scopes: scopes.map(scope => scope.scope) }] : []),
]
const collection = () => ({ tokens: issuedTokensByRole[role], next_cursor: null, capacity: 20 })
const tokenMeta = (id, body) => ({ id, name: body.name, token_hint: `Hint${id}`, scopes: body.scopes, enabled: body.enabled ?? true, expires_at: body.expires_at, allow_ips: body.allow_ips, created_at: 1_800_000_000, updated_at: 1_800_000_000, last_used_at: null, last_used_ip: null, version: 1 })

async function settledDialog(currentPage, title) {
  const dialog = currentPage.getByRole('dialog').filter({ has: currentPage.getByRole('heading', { name: title, exact: true }) })
  await dialog.waitFor({ state: 'visible' })
  await currentPage.waitForFunction(element => {
    if (!element?.isConnected) return false
    for (let ancestor = element; ancestor; ancestor = ancestor.parentElement) {
      if (Number.parseFloat(getComputedStyle(ancestor).opacity) < 0.999) return false
    }
    return true
  }, await dialog.elementHandle())
  // Framer's transform spring can finish after opacity reaches one.
  await currentPage.waitForTimeout(250)
  return dialog
}

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><div id="root"></div><script type="module" src="/.access-token-smoke.tsx"></script>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { BrowserRouter, useHref, useNavigate, useSearchParams } from 'react-router-dom';
    import { HeroUIProvider } from '@heroui/react';
    import { QueryProvider } from './src/lib/query';
    import { appQueryClient } from './src/lib/query/query-client';
    import { RouterNavigationBridge } from './src/lib/router-navigation';
    import { setManagementSessionToken } from './src/lib/api/session-token';
    import { AccessCredentialsPage } from './src/features/api-keys/access-credentials-page';
    import './src/index.css'; import './src/i18n';
    const path = new URLSearchParams(window.location.search).get('path');
    if (path) window.history.replaceState(null, '', path);
    appQueryClient.setDefaultOptions({ queries: { retry: false, refetchOnWindowFocus: false }, mutations: { retry: false } });
    function UiProvider({ children }) { const navigate = useNavigate(); return <HeroUIProvider navigate={navigate} useHref={useHref}>{children}</HeroUIProvider>; }
    function Fixture() { const navigate = useNavigate(); const [params] = useSearchParams(); if (location.pathname === '/login') return <main><h1>CI login fixture</h1><p>{params.get('from')}</p><button onClick={() => { setManagementSessionToken('ci-fresh-session'); navigate(params.get('from') || '/console'); }}>Complete login</button></main>; return <AccessCredentialsPage />; }
    createRoot(document.getElementById('root')).render(<BrowserRouter><UiProvider><QueryProvider><RouterNavigationBridge /><Fixture /></QueryProvider></UiProvider></BrowserRouter>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) { try { if ((await fetch(`${origin}/.access-token-smoke.html`)).ok) { ready = true; break } } catch {} await delay(1000) }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1180, height: 920 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); sessionStorage.setItem('anyflows.management.access-token', 'ci-old-session') })
  const handleApiRoute = async route => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname
    const method = request.method()
    requests.push({ method, path, body: request.postData() ? request.postDataJSON() : undefined, search: url.search })
    const respond = (body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
    if (path === '/api/auth/session' && method === 'GET') return respond({ user: { id: 7, role }, expires_at: 1_900_000_000 })
    if (path === '/api/account/access-tokens' && method === 'GET') return respond(collection())
    if (path === '/api/account/access-tokens/scopes' && method === 'GET') { const scopes = role === 'admin' ? adminScopes : userScopes; return respond({ scopes, presets: preset(scopes), default_expiry_days: 30, max_expiry_days: 365, allow_permanent: true }) }
    if (path === '/api/admin/access-tokens/policy' && method === 'GET') return role === 'admin' ? respond(policy) : respond({ code: 'forbidden' }, 403)
    if (path === '/api/admin/access-tokens/policy' && method === 'PUT') {
      const body = request.postDataJSON()
      assert.equal(body.version, policy.version, 'Policy updates carry the current optimistic-lock version')
      policy = { ...policy, ...body, version: policy.version + 1 }
      return respond(policy)
    }
    if (path === '/api/admin/users/19/access-tokens/revoke-all' && method === 'POST') return respond({ revoked: 2 })
    if (path === '/api/account/access-tokens/verify' && method === 'POST') {
      const body = request.postDataJSON()
      assert.ok(['create', 'update'].includes(body.action))
      assert.ok(Array.isArray(body.payload.scopes))
      if (rejectFirstProof) { rejectFirstProof = false; return respond({ code: 'recent_authentication_required' }, 403) }
      return respond({ proof: 'ci-one-time-proof', expires_at: Math.floor(Date.now() / 1000) + 300 })
    }
    if (path === '/api/account/access-tokens' && method === 'POST') {
      const body = request.postDataJSON()
      assert.equal(body.proof, 'ci-one-time-proof')
      assert.equal(body.enabled, undefined, 'Create DTO has no mutable enabled flag')
      assert.equal(body.version, undefined, 'Create DTO has no version field')
      createCalls += 1
      const token = tokenMeta(createCalls, body)
      issuedTokensByRole[role] = [...issuedTokensByRole[role], token]
      return respond({ token, secret }, 201)
    }
    if (path.match(/^\/api\/account\/access-tokens\/\d+$/) && method === 'PATCH') {
      const id = Number(path.split('/').at(-1))
      const body = request.postDataJSON()
      const current = issuedTokensByRole[role].find(token => token.id === id)
      assert.ok(current, 'The token being updated belongs to the current user')
      assert.equal(body.version, current.version, 'Updates carry the current optimistic-lock version')
      assert.equal(body.proof, undefined, 'Renaming without increasing access does not require proof')
      updateCalls += 1
      issuedTokensByRole[role] = issuedTokensByRole[role].map(token => token.id === id ? { ...tokenMeta(id, body), version: current.version + 1 } : token)
      return respond(issuedTokensByRole[role].find(token => token.id === id))
    }
    if (path.match(/^\/api\/account\/access-tokens\/\d+$/) && method === 'DELETE') {
      revokeCalls += 1
      const id = Number(path.split('/').at(-1))
      issuedTokensByRole[role] = issuedTokensByRole[role].filter(token => token.id !== id)
      return route.fulfill({ status: 204 })
    }
    if (path === '/api/account/access-tokens/revoke-all' && method === 'POST') return respond({ revoked: issuedTokensByRole[role].length })
    if (path === '/api/account/wallet' && method === 'GET') {
      patExplorerAuthorization = request.headers().authorization
      return respond({ code: 'invalid_token' }, 401)
    }
    if (path === '/api/tokens' && method === 'GET') return respond({ tokens: [], next_cursor: null, capacity: 32 })
    if (path === '/api/token-groups' && method === 'GET') return respond({ groups: [], next_cursor: null })
    if (path === '/api/user-routes' && method === 'GET') return respond({ items: [], capacity: 32 })
    if (path === '/api/merchant-pool/routes' && method === 'GET') return respond({ items: [], capacity: 32 })
    if (path === '/api/account/verifications/eligibility' && method === 'GET') return respond({ verified: true, require_verification_for_api: false, require_verification_for_topup: false, enterprise_verified: false, can_apply_for_organization: false })
    return respond({ code: 'unexpected_mock_endpoint' }, 404)
  }
  await page.route(url => url.pathname.startsWith('/api/'), handleApiRoute)
  await page.route(url => url.pathname.startsWith('/v1/'), async route => {
    gatewayRequestCount += 1
    modelKeyHeader = route.request().headers()['x-api-key']
    modelKeyAuthorization = route.request().headers().authorization
    await route.fulfill({ status: 401, contentType: 'application/json', body: JSON.stringify({ error: { code: 'invalid_api_key' } }) })
  })

  await page.goto(`${origin}/.access-token-smoke.html?path=${encodeURIComponent('/console/api-keys?tab=system')}`)
  await page.getByRole('tab', { name: label('tabs.systemTokens'), exact: true }).click()
  await page.getByRole('heading', { name: label('title'), exact: true }).waitFor()
  await page.getByRole('button', { name: label('actions.create'), exact: true }).click()
  await page.getByLabel(label('editor.name'), { exact: true }).fill('CI administrator automation')
  assert.equal(await page.getByRole('button', { name: label('presets.admin_full'), exact: true }).count(), 1)
  await page.getByRole('button', { name: label('presets.admin_full'), exact: true }).click()
  const adminScopeCheckbox = page.getByRole('checkbox', { name: /更新渠道/ })
  assert.equal(await adminScopeCheckbox.isChecked(), true, 'Administrator full preset expands concrete administrative scopes')
  assert.ok(await page.getByRole('checkbox').count() >= adminScopes.length)
  const createButton = page.getByRole('button', { name: label('actions.create'), exact: true }).last()
  await createButton.click()
  await page.getByText(label('errors.recentAuthentication'), { exact: true }).waitFor()
  const draft = await page.evaluate(() => sessionStorage.getItem('anyflows.access-token.pending-draft'))
  assert.equal(draft, null, 'Draft is stored only when the user explicitly resumes authentication')
  await page.getByRole('button', { name: label('actions.reauthenticate'), exact: true }).click()
  await page.waitForURL(url => url.pathname === '/login')
  const returnPath = new URL(page.url()).searchParams.get('from')
  assert.equal(returnPath, '/console/api-keys?tab=system')
  const persistedDraft = await page.evaluate(() => sessionStorage.getItem('anyflows.access-token.pending-draft'))
  assert.ok(persistedDraft)
  assert.equal(persistedDraft.includes(secret), false)
  assert.equal(persistedDraft.includes('password'), false)
  await page.getByRole('button', { name: 'Complete login', exact: true }).click()
  await page.getByLabel(label('editor.name'), { exact: true }).waitFor()
  assert.equal(await page.getByLabel(label('editor.name'), { exact: true }).inputValue(), 'CI administrator automation')
  const recoveredScope = page.getByRole('checkbox', { name: /更新渠道/ })
  assert.equal(await recoveredScope.isChecked(), true)
  assert.equal(await page.evaluate(() => sessionStorage.getItem('anyflows.access-token.pending-draft')), null)
  await page.getByRole('button', { name: label('actions.create'), exact: true }).last().click()
  await page.getByRole('heading', { name: label('issued.title'), exact: true }).waitFor()
  assert.equal(await page.getByLabel(label('issued.secret'), { exact: true }).inputValue(), secret)
  const createRequest = requests.findLast(item => item.method === 'POST' && item.path === '/api/account/access-tokens')
  assert.ok(createRequest)
  assert.equal(createRequest.body.proof, 'ci-one-time-proof')
  assert.equal(createRequest.body.scopes.includes('admin:channel:write'), true)
  assert.equal(createRequest.search.includes(secret), false)
  assert.equal(await page.evaluate(secretValue => [...Object.values(sessionStorage), ...Object.values(localStorage)].some(value => value.includes(secretValue)), secret), false)
  assert.equal(gatewayRequestCount, 0, 'The credential console does not send PATs or model keys to model gateway APIs')
  const patProbe = await page.evaluate(async () => {
    const { runApiExplorerOperation } = await import('/src/features/api-explorer/api-explorer-api.ts')
    return runApiExplorerOperation({ operation_id: 'getAccountWallet', method: 'GET', path: '/api/account/wallet', tag: 'wallet', summary: 'Wallet', auth_mode: 'session', supports_access_token: true, required_scopes: ['billing:read'], access_scope: 'session', debuggable: true }, { pathParams: {}, queryParams: {}, useAccessToken: true, accessToken: 'af_pat_fixture_unauthorized' })
  })
  assert.equal(patProbe.status, 401)
  assert.equal(patExplorerAuthorization, 'Bearer af_pat_fixture_unauthorized', 'API Explorer sends the entered PAT as the bearer token')
  assert.equal(await page.evaluate(() => sessionStorage.getItem('anyflows.management.access-token')), 'ci-fresh-session', 'A PAT 401 does not clear the interactive browser session')
  const modelKeyProbe = await page.evaluate(async () => {
    const { runApiExplorerOperation } = await import('/src/features/api-explorer/api-explorer-api.ts')
    return runApiExplorerOperation({ operation_id: 'createChatCompletion', method: 'POST', path: '/v1/chat/completions', tag: 'models', summary: 'Chat', auth_mode: 'api_key', access_scope: 'gateway', debuggable: true }, { pathParams: {}, queryParams: {}, apiKey: 'sk-model-key-fixture', body: '{"model":"fixture"}' })
  })
  assert.equal(modelKeyProbe.status, 401)
  assert.equal(gatewayRequestCount, 1, 'Only the explicit model API key probe reaches the gateway')
  assert.equal(modelKeyHeader, 'sk-model-key-fixture', 'Model API requests keep their independent x-api-key credential')
  assert.equal(modelKeyAuthorization, undefined, 'The model API key probe does not carry the browser session or a PAT')
  assert.equal(await page.evaluate(() => sessionStorage.getItem('anyflows.management.access-token')), 'ci-fresh-session')
  await settledDialog(page, label('issued.title'))
  await page.screenshot({ path: resolve(screenshots, 'access-token-once-issued.png'), fullPage: true, animations: 'disabled' })
  await page.getByRole('button', { name: label('issued.done'), exact: true }).click()
  await page.getByRole('button', { name: label('actions.refresh'), exact: true }).click()
  await page.getByText('CI administrator automation', { exact: true }).waitFor()
  const proofsBeforeRename = requests.filter(item => item.path === '/api/account/access-tokens/verify').length
  await page.getByRole('button', { name: label('actions.edit'), exact: true }).click()
  await page.getByLabel(label('editor.name'), { exact: true }).fill('CI renamed administrator automation')
  await page.getByRole('button', { name: label('actions.save'), exact: true }).click()
  await page.getByText('CI renamed administrator automation', { exact: true }).waitFor()
  assert.equal(updateCalls, 1)
  assert.equal(requests.filter(item => item.path === '/api/account/access-tokens/verify').length, proofsBeforeRename, 'A rename does not request elevated proof')
  await page.getByRole('button', { name: label('actions.revoke'), exact: true }).click()
  await page.getByRole('heading', { name: label('revoke.title'), exact: true }).waitFor()
  await page.getByRole('button', { name: label('actions.revoke'), exact: true }).last().click()
  await page.getByText(label('empty.title'), { exact: true }).waitFor()
  assert.equal(revokeCalls, 1)
  await page.getByRole('tab', { name: label('tabs.modelKeys'), exact: true }).click()
  await page.getByRole('heading', { name: commonLabel('apiKeys.title'), exact: true }).waitFor()
  assert.ok(requests.some(item => item.path === '/api/tokens' && item.method === 'GET'))
  assert.equal(gatewayRequestCount, 1, 'Credential editing, revocation and tab changes send no additional gateway requests')
  assert.equal(await page.getByLabel(label('issued.secret'), { exact: true }).count(), 0, 'Switching credential tabs discards the one-time PAT secret')

  role = 'user'
  userContext = await browser.newContext({ viewport: { width: 1180, height: 920 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  const userPage = await userContext.newPage()
  userPage.setDefaultTimeout(20000)
  userPage.on('pageerror', error => errors.push(error.stack || error.message))
  await userPage.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh'); sessionStorage.setItem('anyflows.management.access-token', 'ci-user-session') })
  await userPage.route(url => url.pathname.startsWith('/api/'), handleApiRoute)
  await userPage.goto(`${origin}/.access-token-smoke.html?path=${encodeURIComponent('/console/api-keys?tab=system')}`)
  await userPage.getByRole('heading', { name: label('title'), exact: true }).waitFor()
  assert.equal(await userPage.getByRole('heading', { name: label('admin.title'), exact: true }).count(), 0, 'User PATs do not expose administrator policy controls')
  await userPage.getByRole('button', { name: label('actions.create'), exact: true }).click()
  assert.equal(await userPage.getByRole('button', { name: label('presets.admin_full'), exact: true }).count(), 0, 'User PATs cannot select administrator scopes')
  assert.equal(await userPage.getByRole('checkbox', { name: /更新渠道/ }).count(), 0, 'Administrator scopes are absent from the user catalog')
  await settledDialog(userPage, label('editor.createTitle'))
  await userPage.screenshot({ path: resolve(screenshots, 'access-token-user-scopes.png'), fullPage: true, animations: 'disabled' })

  await userPage.setViewportSize({ width: 390, height: 844 })
  const mobileDialog = await settledDialog(userPage, label('editor.createTitle'))
  const mobileLayout = await mobileDialog.evaluate(dialog => {
    const bounds = dialog.getBoundingClientRect()
    const scrollable = [dialog, ...dialog.querySelectorAll('*')].find(element => {
      const style = getComputedStyle(element)
      return /auto|scroll/.test(style.overflowY) && element.clientHeight > 0 && element.scrollHeight > element.clientHeight + 1
    })
    let canScroll = false
    if (scrollable) {
      const before = scrollable.scrollTop
      scrollable.scrollTop = 0
      scrollable.scrollTop = 50
      canScroll = scrollable.scrollTop > 0
      scrollable.scrollTop = before
    }
    return { pageWidth: document.documentElement.scrollWidth, viewportWidth: innerWidth, viewportHeight: innerHeight,
      left: bounds.left, right: bounds.right, top: bounds.top, bottom: bounds.bottom, canScroll }
  })
  assert.ok(mobileLayout.pageWidth <= mobileLayout.viewportWidth, 'The mobile editor causes no horizontal document overflow')
  assert.ok(mobileLayout.left >= -1 && mobileLayout.right <= mobileLayout.viewportWidth + 1, 'The mobile dialog stays inside the viewport width')
  assert.ok(mobileLayout.top >= -1 && mobileLayout.bottom <= mobileLayout.viewportHeight + 1, 'The mobile dialog stays inside the viewport height')
  assert.equal(mobileLayout.canScroll, true, 'The mobile dialog body remains vertically scrollable')
  await settledDialog(userPage, label('editor.createTitle'))
  await userPage.screenshot({ path: resolve(screenshots, 'access-token-user-scopes-mobile.png'), fullPage: true, animations: 'disabled' })

  assert.deepEqual(errors, [])
  console.log('System access token browser regression passed: step-up recovery, one-time secret, admin/user scope isolation, rename, revoke and model-key separation.')
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ requests, errors, body: page ? await page.locator('body').innerText().catch(() => '') : '' }))
  throw error
} finally {
  if (userContext) await userContext.close()
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
