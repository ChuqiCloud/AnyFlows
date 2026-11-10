import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { readFile, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

assert.equal(process.env.CI, 'true', 'OAuth invitation browser checks run in remote CI')
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Use the remote Playwright installation')
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const directory = resolve(root, 'web-next')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const translations = JSON.parse(await readFile(resolve(directory, 'src/i18n/locales/zh.json'), 'utf8'))
const label = key => key.split('.').reduce((node, part) => node[part], translations)
const html = resolve(directory, '.oauth-invitation-smoke.html')
const entry = resolve(directory, '.oauth-invitation-smoke.tsx')
const port = 4176
const origin = `http://127.0.0.1:${port}`
const code = 'af-AAAAAAAAAAAAAAAAAAAAAA'
const editedCode = 'af-BBBBBBBBBBBBBBBBBBBBBB'
const starts = []
const errors = []
let passwordEnabled = true
let registrationEnabled = true
let rejectStart = false
let server
let browser
let serverOutput = ''
const htmlSource = '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>OAuth invitation CI</title><div id="root"></div><script type="module" src="/.oauth-invitation-smoke.tsx"></script></html>'

try {
  await writeFile(html, htmlSource)
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
    const path = new URLSearchParams(window.location.search).get('path');
    if (path) window.history.replaceState(null, '', path);
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
    try { if ((await fetch(`${origin}/.oauth-invitation-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  const page = await browser.newPage({ viewport: { width: 1100, height: 1000 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(20000)
  page.on('pageerror', error => errors.push(error.stack || error.message))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    sessionStorage.removeItem('anyflows.management.access-token')
    localStorage.removeItem('auth_principal')
    sessionStorage.removeItem('auth_principal')
  })
  await page.route(url => ['/register', '/login'].includes(url.pathname), route => route.fulfill({ contentType: 'text/html', body: htmlSource }))
  await page.route(url => url.pathname === '/.oauth-started.html', route => route.fulfill({ contentType: 'text/html', body: '<h1>OAuth started</h1>' }))
  await page.route(url => url.pathname === '/ws/token', route => route.fulfill({ status: 404, contentType: 'application/json', body: '{"code":"not_found"}' }))
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const url = new URL(request.url())
    if (url.pathname.endsWith('/start')) {
      starts.push({ path: url.pathname, invite: url.searchParams.get('invite_code'), method: request.method() })
      await route.fulfill({
        status: rejectStart ? 503 : 200, contentType: 'application/json',
        body: JSON.stringify(rejectStart ? { code: 'unavailable' } : { authorization_url: `${origin}/.oauth-started.html` }),
      })
      return
    }
    const data = url.pathname === '/api/setup/status' ? { setup_required: false } : url.pathname === '/api/site' ? {
      site_name: 'CI AnyFlows', public_base_url: origin,
      brand: { logo_url: null, tagline: '', description: '' },
      navigation: { header_links: [], footer_groups: [], sidebar_links: [] },
      balance_display: { mode: 'quota', unit_name: 'quota', unit_symbol: 'quota', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 },
      authentication: {
        password_login_enabled: passwordEnabled, registration_enabled: registrationEnabled,
        registration_email_required: false, turnstile_site_key: null,
        oauth_providers: [{ id: 'github', display_name: 'GitHub' }, { id: 'custom_enterprise', display_name: 'Enterprise' }],
      },
    } : null
    await route.fulfill({ status: data ? 200 : 401, contentType: 'application/json', body: JSON.stringify(data ?? { code: 'invalid_session' }) })
  })
  const visit = path => page.goto(`${origin}/.oauth-invitation-smoke.html?path=${encodeURIComponent(path)}`)
  const github = () => page.getByRole('button', { name: label('auth.login.oauth.github'), exact: true })
  const expectStart = async (providerPath, invite) => {
    await page.waitForURL(`${origin}/.oauth-started.html`)
    assert.deepEqual(starts.at(-1), { path: providerPath, invite, method: 'POST' })
  }

  await visit(`/register?invite=${code}`)
  await page.locator('#registration-invite-code').waitFor()
  assert.equal(await page.locator('#registration-invite-code').inputValue(), code)
  await github().click()
  await expectStart('/api/auth/oauth/github/start', code)

  await visit(`/register?invite=${code}`)
  await page.locator('#registration-invite-code').fill(editedCode)
  const customLabel = label('auth.login.oauth.custom').replace('{{provider}}', 'Enterprise')
  await page.getByRole('button', { name: customLabel, exact: true }).click()
  await expectStart('/api/auth/oauth/custom/custom_enterprise/start', editedCode)

  passwordEnabled = false
  await visit(`/register?invite=${code}`)
  await page.locator('#registration-invite-code').waitFor()
  assert.equal(await page.locator('#registration-username, #registration-password').count(), 0)
  await page.locator('#registration-invite-code').fill(editedCode)
  await page.locator('form a[href^="/login"]').click()
  await page.waitForURL(`${origin}/login?invite=${editedCode}`)
  await github().waitFor()
  const registerLink = page.locator('form a[href^="/register"]')
  assert.equal(await registerLink.getAttribute('href'), `/register?invite=${editedCode}`)
  await registerLink.click()
  await page.waitForURL(`${origin}/register?invite=${editedCode}`)
  await github().click()
  await expectStart('/api/auth/oauth/github/start', editedCode)

  await visit(`/register?invite=${code}`)
  await page.locator('#registration-invite-code').fill('invalid')
  const beforeInvalid = starts.length
  await github().click()
  await page.getByText(label('registration.validation.inviteCode'), { exact: true }).waitFor()
  assert.equal(starts.length, beforeInvalid)
  await page.locator('#registration-invite-code').fill(code)
  rejectStart = true
  await github().click()
  await page.getByRole('alert').waitFor()
  assert.equal(await github().isEnabled(), true)
  rejectStart = false
  await github().click()
  await expectStart('/api/auth/oauth/github/start', code)

  for (const path of ['/register', '/login']) {
    for (const query of [`invite=${code}&invite=${code}`, 'invite=invalid']) {
      const before = starts.length
      await visit(`${path}?${query}`)
      await page.getByRole('heading', { name: label('registration.validation.inviteCode'), exact: true }).waitFor()
      assert.equal(await github().count(), 0)
      assert.equal(starts.length, before)
    }
  }
  registrationEnabled = false
  await visit(`/register?invite=${code}`)
  await page.getByRole('heading', { name: label('registration.status.disabledTitle'), exact: true }).waitFor()
  assert.equal(await github().count(), 0)
  assert.deepEqual(errors, [])
  console.log('OAuth invitation browser checks passed: current code, custom provider, OAuth-only registration, navigation, invalid links, error retry and closed registration.')
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ errors, starts }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
