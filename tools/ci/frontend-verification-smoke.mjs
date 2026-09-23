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
const html = resolve(directory, '.verification-smoke.html')
const entry = resolve(directory, '.verification-smoke.tsx')
const isNext = frontend === 'web-next'
const errors = []
let server
let browser
let serverOutput = ''

try {
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Verification CI</title><div id="root"></div><script type="module" src="/.verification-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { AccountVerificationPage } from './src/features/account-verification/account-verification-page';
    import { PlatformEntitlementEditor } from './src/features/platform-organizations/platform-entitlement-editor';
    const mode = new URLSearchParams(location.search).get('mode');
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '16px', maxWidth: '1100px', margin: 'auto' }}>
          {mode === 'entitlement' ? <PlatformEntitlementEditor organizationId={1} name="Example Enterprise" onClose={() => {}} /> : <AccountVerificationPage admin={mode === 'admin'} />}
        </main>${isNext ? '</HeroUIProvider>' : ''}
      </QueryClientProvider>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  server.stderr.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.verification-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1_000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(15_000)
  page.on('pageerror', (error) => errors.push(error.message))
  await page.addInitScript(() => localStorage.setItem('anyflows.locale', 'zh'))
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a9mQAAAAASUVORK5CYII=', 'base64')
  let submitted = 0
  let reviewed = 0
  let downloads = 0
  let entitlementWrites = 0
  const cases = Array.from({ length: 27 }, (_, index) => ({ id: 27 - index, user_id: 1, kind: 'enterprise', subject_name: `Enterprise ${27 - index}`, summary: 'Historical application', status: 5, version: 2, review_reason: 'Supplement required', reviewer_user_id: 2, created_at: 1_780_000_000, updated_at: 1_780_000_010 }))
  let entitlement = { version: 2, sso_enabled: true, team_management_enabled: true, custom_roles_enabled: true, seat_limit: 10, team_limit: 5, organization_key_limit: 5, audit_retention_days: 90, valid_from: 1_700_000_000, valid_until: 2_000_000_000, grace_until: null }
  await page.route((url) => url.pathname.startsWith('/api/'), async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname
    let body
    if (path.endsWith('/eligibility')) body = { enterprise_verified: reviewed > 0, can_apply_for_organization: reviewed > 0 }
    else if (path.includes('/entitlement')) {
      if (request.method() === 'PATCH') {
        const update = request.postDataJSON()
        assert.equal(update.expected_version, entitlement.version)
        assert.ok(update.reason.trim())
        assert.equal(update.sso_enabled, entitlementWrites > 0)
        entitlement = { ...entitlement, ...update, version: entitlement.version + 1 }
        entitlementWrites += 1
      }
      body = entitlement
    } else if (path.includes('/materials/')) {
      downloads += 1
      await route.fulfill({ contentType: 'image/png', body: png })
      return
    } else if (path.endsWith('/decision')) {
      const update = request.postDataJSON()
      assert.equal(update.expected_version, cases[0].version)
      assert.equal(update.status, 4)
      cases[0] = { ...cases[0], status: 4, version: cases[0].version + 1 }
      reviewed += 1
      body = cases[0]
    } else if (request.method() === 'POST') {
      assert.match(request.headers()['content-type'], /^multipart\/form-data; boundary=/)
      assert.ok(request.postDataBuffer().includes(Buffer.from('name="file:0"')))
      assert.ok(request.postDataBuffer().includes(Buffer.from('"kind":"enterprise"')))
      cases.unshift({ ...cases[0], id: 28, subject_name: 'New Enterprise', status: 1, version: 1, review_reason: null, reviewer_user_id: null })
      submitted += 1
      body = cases[0]
    } else if (/\/\d+$/.test(path)) {
      const record = cases.find((item) => item.id === Number(path.split('/').at(-1)))
      assert.ok(record)
      body = { case: record, materials: [{ id: 1, case_id: record.id, kind: 'business_document', file_name: 'license.png', content_type: 'image/png', size_bytes: png.length }] }
    } else {
      assert.ok(['/api/account/verifications', '/api/admin/account-verifications'].includes(path), path)
      const before = Number(url.searchParams.get('before') ?? Infinity)
      const status = url.searchParams.get('status')
      const rows = cases.filter((item) => item.id < before && (!status || item.status === Number(status))).slice(0, 25)
      body = { cases: rows, next_cursor: rows.length === 25 ? rows.at(-1).id : null }
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) })
  })
  const home = `http://127.0.0.1:${port}/.verification-smoke.html`
  await page.goto(home)
  await page.getByText('Enterprise 27', { exact: false }).first().waitFor()
  assert.equal(await page.getByRole('button', { name: '查看详情', exact: true }).count(), 25)
  assert.equal(downloads, 0)
  await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
  await page.getByText('license.png', { exact: true }).waitFor()
  assert.equal(downloads, 0, 'Opening record metadata must not download file contents')
  await page.getByRole('button', { name: '预览', exact: true }).click()
  await page.locator('img[alt="license.png"]').waitFor()
  await page.waitForFunction(() => [...document.images].every((img) => img.complete && img.naturalWidth > 0))
  assert.equal(downloads, 1)
  await page.getByRole('button', { name: '加载更多', exact: true }).click()
  await page.getByText('Enterprise 1 ', { exact: false }).last().waitFor()
  assert.equal(await page.getByRole('button', { name: '查看详情', exact: true }).count(), 27)
  await page.getByRole('tab', { name: '企业认证', exact: true }).click()
  await page.getByLabel('认证主体名称', { exact: true }).fill('New Enterprise')
  await page.getByLabel('认证说明', { exact: true }).fill('Enterprise identity review')
  await page.locator('input[type="file"]').setInputFiles({ name: 'license.png', mimeType: 'image/png', buffer: png })
  await page.getByRole('button', { name: '提交认证', exact: true }).click()
  await page.getByText('New Enterprise', { exact: false }).first().waitFor()
  assert.equal(submitted, 1)
  await page.goto(`${home}?mode=admin`)
  await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
  await page.getByRole('button', { name: '通过认证', exact: true }).click()
  await page.getByText('已通过', { exact: true }).waitFor()
  assert.equal(reviewed, 1)
  await page.goto(home)
  await page.getByRole('link', { name: '申请企业空间', exact: true }).waitFor()
  await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
  await page.getByRole('button', { name: '预览', exact: true }).click()
  await page.locator('img[alt="license.png"]').waitFor()
  assert.equal(downloads, 2, 'Approved materials must remain accessible')
  await page.goto(`${home}?mode=entitlement`)
  const sso = page.getByRole('checkbox', { name: 'SSO 与域名验证', exact: true })
  await sso.uncheck()
  await page.getByLabel('变更原因（必填，将记录审计）', { exact: true }).fill('Disable SSO for maintenance')
  await page.getByRole('button', { name: '保存授权', exact: true }).click()
  await page.getByRole('button', { name: '继续编辑', exact: true }).click()
  await sso.check()
  await page.getByLabel('变更原因（必填，将记录审计）', { exact: true }).fill('Reenable SSO after maintenance')
  await page.getByRole('button', { name: '保存授权', exact: true }).click()
  await page.getByText('企业授权已更新。', { exact: true }).waitFor()
  assert.equal(entitlementWrites, 2)
  for (const theme of ['dark', 'light']) {
    await page.evaluate((mode) => { document.documentElement.classList.remove('dark', 'light'); document.documentElement.classList.add(mode) }, theme)
    for (const [name, width, height] of [['desktop', 1280, 960], ['mobile', 390, 844]]) {
      await page.setViewportSize({ width, height })
      await page.evaluate(() => document.fonts.ready)
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${frontend} overflowed at ${width}px`)
      const screenshot = await page.screenshot({ fullPage: true, animations: 'disabled' })
      console.log(`ANYFLOWS_VERIFICATION_SCREENSHOT_${frontend}_${theme}_${name}_BEGIN`)
      console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
      console.log(`ANYFLOWS_VERIFICATION_SCREENSHOT_${frontend}_${theme}_${name}_END`)
    }
  }
  assert.deepEqual(errors, [])
  console.log(`Verification browser checks passed for ${frontend}: cursor pagination, deferred previews, multipart upload, approval, retained materials, eligibility and repeated entitlement changes.`)
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
