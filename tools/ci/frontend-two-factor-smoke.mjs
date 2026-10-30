import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { writeFile, unlink } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const frontend = process.argv[2]
assert.ok(['web', 'web-next'].includes(frontend))
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Run browser checks in the remote CI environment')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const directory = resolve(root, frontend)
const port = frontend === 'web' ? 4173 : 4174
const html = resolve(directory, '.two-factor-smoke.html')
const entry = resolve(directory, '.two-factor-smoke.tsx')
const config = resolve(directory, '.two-factor-smoke.config.ts')
const errors = []
const unexpected = []
const pendingRequests = new Set()
let server
let browser
let page
let serverOutput = ''

try {
  await writeFile(config, `
    import { defineConfig, mergeConfig } from 'vite';
    import base from './vite.config';
    export default defineConfig(async (env) => {
      const resolved = await (typeof base === 'function' ? base(env) : base);
      return mergeConfig(resolved, { optimizeDeps: { entries: ['.two-factor-smoke.html'] } });
    });
  `)
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>TOTP CI</title><div id="root"></div><script type="module" src="/.two-factor-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${frontend === 'web-next' ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { ProfilePage } from './src/features/profile/profile-page';
    import { subscribeManagementSessionInvalidated } from './src/lib/api/session-token';
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    window.__sessionInvalidated = 0;
    window.__passwordChanged = 0;
    subscribeManagementSessionInvalidated(() => { window.__sessionInvalidated += 1; });
    window.__cacheSnapshot = () => JSON.stringify({ queries: client.getQueryCache().getAll().map(q => q.state), mutations: client.getMutationCache().getAll().map(m => m.state) });
    const root = createRoot(document.getElementById('root')!);
    const render = (visible = true) => root.render(
      <QueryClientProvider client={client}>
        ${frontend === 'web-next' ? '<HeroUIProvider>' : ''}
        <main style={{ padding: '16px', maxWidth: '1100px', margin: 'auto' }}>
          {visible ? <ProfilePage onPasswordChanged={() => { window.__passwordChanged += 1; }} /> : <p>Profile closed</p>}
        </main>
        ${frontend === 'web-next' ? '</HeroUIProvider>' : ''}
      </QueryClientProvider>
    );
    window.__hideProfile = () => render(false);
    render();
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--config', config, '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  server.stderr.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.two-factor-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1_000)
  }
  assert.ok(ready, serverOutput)
  const warmup = await fetch(`http://127.0.0.1:${port}/.two-factor-smoke.tsx`, { signal: AbortSignal.timeout(180_000) })
  assert.ok(warmup.ok, await warmup.text())
  console.log(`TOTP profile fixture ready for ${frontend}.`)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(30_000)
  page.setDefaultNavigationTimeout(120_000)
  page.on('pageerror', (error) => errors.push(error.message))
  page.on('request', request => pendingRequests.add(request))
  page.on('requestfinished', request => pendingRequests.delete(request))
  page.on('requestfailed', request => pendingRequests.delete(request))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    sessionStorage.setItem('anyflows.management.access-token', 'totp-fixture-session')
  })
  let now = Math.floor(Date.now() / 1000)
  await page.clock.install({ time: new Date(now * 1000) })
  const secret = 'JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP'
  const uri = `otpauth://totp/AnyFlows:test?secret=${secret}&issuer=AnyFlows`
  const backups = Array.from({ length: 10 }, (_, index) => `backup-fixture-${index}`)
  const password = 'fixture-current-password'
  let state = { enabled: false, state: 'disabled', pending_created_at: null, pending_expires_at: null, bound_at: null, last_verified_at: null }
  let preparations = 0
  let confirmations = 0
  await page.route((url) => url.pathname.startsWith('/api/'), async (route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    let body
    let status = 200
    if (path === '/api/account/two-factor') {
      if (request.method() === 'POST') {
        const input = request.postDataJSON()
        assert.equal(input.current_password, password)
        if (input.totp_code === undefined) {
          preparations += 1
          state = { ...state, state: 'pending', pending_created_at: now, pending_expires_at: now + 600 }
          body = { ...state, secret, otpauth_uri: uri, backup_codes: [] }
        } else {
          confirmations += 1
          if (input.totp_code !== '123456') {
            status = 400
            body = { code: 'two_factor_enrollment_invalid', message: 'Invalid authenticator setup code' }
          } else {
            state = { enabled: true, state: 'enabled', pending_created_at: null, pending_expires_at: null, bound_at: now, last_verified_at: null }
            body = { ...state, secret, otpauth_uri: uri, backup_codes: backups }
          }
        }
      } else body = state
    } else if (path === '/api/account/profile') {
      body = { id: 1, username: 'fixture-owner', email: null, role: 'user', notifications: { email_product_updates: false, email_usage_alerts: false, balance_alert_enabled: false, balance_alert_threshold: null, effective_balance_alert_threshold: 100, subscription_alert_enabled: false, subscription_remaining_percent: 10 } }
    } else if (path === '/api/account/passkeys') body = { items: [] }
    else if (path === '/api/account/notifications') body = { entries: [], next_cursor: null, unread_count: 0 }
    else { unexpected.push(`${request.method()} ${path}`); status = 404; body = {} }
    await route.fulfill({ status, contentType: 'application/json', headers: { 'Cache-Control': 'no-store' }, body: JSON.stringify(body) })
  })
  const card = page.getByRole('region', { name: '二次验证（TOTP）' })
  const qr = card.locator('svg title').filter({ hasText: '身份验证器绑定二维码' })
  const currentPassword = card.getByLabel('当前密码', { exact: true })
  const code = card.getByLabel('身份验证器验证码', { exact: true })
  const assertNoCachedMaterials = async () => {
    await page.waitForFunction((sensitive) => sensitive.every(value => !window.__cacheSnapshot().includes(value)), [secret, uri, password, ...backups])
  }
  const assertSession = async () => {
    assert.deepEqual(await page.evaluate(() => [window.__sessionInvalidated, window.__passwordChanged, sessionStorage.getItem('anyflows.management.access-token')]), [0, 0, 'totp-fixture-session'])
  }
  const assertResponsive = async () => {
    for (const width of [1280, 390]) {
      await page.setViewportSize({ width, height: 960 })
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${frontend} overflowed at ${width}px`)
    }
    await page.setViewportSize({ width: 1280, height: 960 })
  }

  await page.goto(`http://127.0.0.1:${port}/.two-factor-smoke.html`)
  await currentPassword.fill(password)
  await card.getByRole('button', { name: '生成绑定二维码', exact: true }).click()
  await qr.waitFor({ state: 'attached' })
  assert.equal(await currentPassword.inputValue(), '')
  assert.equal(await card.getByText('一次性备用码', { exact: true }).count(), 0)
  await card.getByText(/^开始绑定：/).waitFor()
  await card.getByText(/^有效期至：/).waitFor()
  await assertNoCachedMaterials()
  await assertResponsive()

  await currentPassword.fill(password)
  await code.fill('000000')
  await card.getByRole('button', { name: '确认绑定', exact: true }).click()
  await card.getByRole('alert').filter({ hasText: '验证码不正确或绑定已过期' }).waitFor()
  assert.equal(await qr.count(), 1)
  await assertSession()
  await assertNoCachedMaterials()

  await page.reload()
  await code.waitFor()
  assert.equal(await qr.count(), 0, 'Reload must not recover secret material from cache')
  await currentPassword.fill(password)
  await card.getByRole('button', { name: '重新生成二维码', exact: true }).click()
  await qr.waitFor({ state: 'attached' })
  now += 601
  await page.clock.fastForward(601_000)
  await card.getByText('绑定已过期', { exact: true }).waitFor()
  assert.equal(await qr.count(), 0)
  assert.equal(await card.getByText(secret, { exact: true }).count(), 0)
  assert.equal(await code.count(), 0)
  await currentPassword.fill(password)
  await card.getByRole('button', { name: '重新生成二维码', exact: true }).click()
  await qr.waitFor({ state: 'attached' })
  await currentPassword.fill(password)
  await code.fill('123456')
  await card.getByRole('button', { name: '确认绑定', exact: true }).click()
  await card.getByText('二次验证已绑定', { exact: true }).waitFor()
  assert.equal(await qr.count(), 0)
  assert.equal(await card.getByText(secret, { exact: true }).count(), 0)
  for (const backup of backups) await card.getByText(backup, { exact: true }).waitFor()
  await card.getByText(/^绑定时间：/).waitFor()
  await card.getByText('最近登录验证：暂无记录', { exact: true }).waitFor()
  await assertSession()
  await assertNoCachedMaterials()
  await assertResponsive()
  await card.getByRole('button', { name: '已保存备用码', exact: true }).click()
  assert.equal(await card.getByText(backups[0], { exact: true }).count(), 0)
  state.last_verified_at = now + 30
  await page.reload()
  const verified = card.getByText(/^最近登录验证：/)
  await verified.waitFor()
  assert.ok(!(await verified.textContent()).includes('暂无记录'))
  assert.equal(await card.getByText('一次性备用码', { exact: true }).count(), 0)
  await page.evaluate(() => window.__hideProfile())
  await page.getByText('Profile closed', { exact: true }).waitFor()
  await assertNoCachedMaterials()
  assert.equal(preparations, 3)
  assert.equal(confirmations, 2)
  assert.deepEqual(unexpected, [])
  assert.deepEqual(errors, [])
  console.log(`TOTP browser checks passed for ${frontend}: staged binding, recovery, expiry, session continuity, timestamps, responsive layout, and sensitive cache cleanup.`)
} catch (error) {
  console.error(serverOutput)
  const documentState = page ? await page.evaluate(() => ({ readyState: document.readyState, text: document.body?.innerText.slice(0, 2000) })).catch(() => null) : null
  console.error(JSON.stringify({ browserErrors: errors, unexpectedRequests: unexpected, pendingRequests: [...pendingRequests].map(request => new URL(request.url()).pathname), documentState }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry), unlink(config)])
}
