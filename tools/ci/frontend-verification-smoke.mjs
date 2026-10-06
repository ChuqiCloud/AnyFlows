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
    import { VerificationSettingsPanel } from './src/features/account-verification/verification-settings-panel';
    import { ModelProviderCatalogPanel } from './src/features/model-management/model-provider-catalog-panel';
    import { resolveProviderLogo, findProviderLogo } from './src/components/brand/model-logos';
    if (resolveProviderLogo('Jina', 'deepseek')?.name !== 'Jina') throw new Error('Configured logo must take priority');
    if (resolveProviderLogo(null, 'private-provider', 'DeepSeek')?.name !== 'DeepSeek') throw new Error('Display name must identify logo');
    if (findProviderLogo('google-vertex')?.name !== 'Google Vertex AI') throw new Error('Compound aliases must identify logo');
    const mode = new URLSearchParams(location.search).get('mode');
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '16px', maxWidth: '1100px', margin: 'auto' }}>
          {mode === 'settings' ? <VerificationSettingsPanel /> : mode === 'catalog' ? <ModelProviderCatalogPanel /> : <AccountVerificationPage admin={mode === 'admin'} />}
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
  const capturePage = async (name) => {
    for (const [viewport, width, height] of [['desktop', 1280, 960], ['mobile', 390, 844]]) {
      await page.setViewportSize({ width, height })
      await page.evaluate(() => document.fonts.ready)
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${name} overflowed at ${width}px`)
      console.log(`ANYFLOWS_SETTINGS_SCREENSHOT_${frontend}_${name}_${viewport}_BEGIN`)
      console.log((await page.screenshot({ fullPage: true, animations: 'disabled' })).toString('base64').match(/.{1,120}/g).join('\n'))
      console.log(`ANYFLOWS_SETTINGS_SCREENSHOT_${frontend}_${name}_${viewport}_END`)
    }
    await page.setViewportSize({ width: 1280, height: 960 })
  }
  await page.addInitScript(() => localStorage.setItem('anyflows.locale', 'zh'))
  const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a9mQAAAAASUVORK5CYII=', 'base64')
  let submitted = 0
  let reviewed = 0
  let downloads = 0
  let providers = ['manual']
  let settings = { source: 'database', manual_enabled: true, individual_manual_enabled: true, enterprise_manual_enabled: true, individual_reason_required: true, enterprise_reason_required: true, enabled: false, app_id: 'test-app', private_key_configured: true, public_key_configured: true, gateway_url: 'https://openapi.alipay.com/gateway.do', biz_code: 'FACE', timeout_secs: 10, version: 1 }
  let settingsWrites = 0
  const catalog = [{ provider_key: 'custom-ai', display_name: 'Custom AI', logo: 'OpenAI', aliases: [], enabled: true, sort_order: 0, version: 1 }]
  let catalogWrites = 0
  const cases = Array.from({ length: 27 }, (_, index) => ({ id: 27 - index, user_id: 1, kind: 'enterprise', provider: 'manual', document_country: 'CN', document_type: 'business_registration', document_number_masked: '91****43', subject_name: `Enterprise ${27 - index}`, summary: 'Historical application', status: 5, version: 2, review_reason: 'Supplement required', reviewer_user_id: 2, created_at: 1_780_000_000, updated_at: 1_780_000_010 }))
  await page.route((url) => url.pathname.startsWith('/api/'), async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname
    let body
    if (path.endsWith('/eligibility')) body = { enterprise_verified: reviewed > 0, can_apply_for_organization: reviewed > 0, providers, individual_providers: [...(settings.individual_manual_enabled ? ['manual'] : []), ...(settings.enabled ? ['alipay'] : [])], enterprise_providers: settings.enterprise_manual_enabled ? ['manual'] : [], individual_reason_required: settings.individual_reason_required, enterprise_reason_required: settings.enterprise_reason_required }
    else if (path === '/api/admin/account-verification-settings') {
      if (request.method() === 'PUT') {
        const update = request.postDataJSON()
        assert.equal(update.expected_version, settings.version)
        assert.equal(update.private_key, undefined)
        assert.equal(update.public_key, undefined)
        settings = { ...settings, ...update, version: settings.version + 1 }
        providers = [...(settings.individual_manual_enabled || settings.enterprise_manual_enabled ? ['manual'] : []), ...(settings.enabled ? ['alipay'] : [])]
        settingsWrites += 1
      }
      body = settings
    } else if (path.includes('/model-provider-catalog')) {
      if (request.method() === 'PUT') {
        const update = request.postDataJSON()
        assert.equal(update.expected_version, 0)
        assert.equal(update.display_name, 'Example Provider')
        catalog.push({ ...update, provider_key: path.split('/').at(-1), version: 1 })
        catalogWrites += 1
      }
      body = { providers: catalog }
    }
    else if (path.includes('/materials/')) {
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
      assert.ok(request.postDataBuffer().includes(Buffer.from('"document_country":"CN"')))
      assert.ok(request.postDataBuffer().includes(Buffer.from('"document_type":"business_registration"')))
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
  const providerSelect = page.getByRole('combobox', { name: '认证方式', exact: true })
  const documentSelect = page.getByRole('combobox', { name: '证件类型', exact: true })
  assert.deepEqual(await providerSelect.locator('option').evaluateAll((options) => options.map((option) => option.value)), ['manual'])
  await documentSelect.selectOption('passport')
  assert.equal(await documentSelect.inputValue(), 'passport')
  await page.getByLabel('证件签发国家或地区代码', { exact: true }).fill('BR')
  assert.equal(await page.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'BR')
  await page.getByLabel('证件签发国家或地区代码', { exact: true }).fill('CN')
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
  await page.waitForFunction(() => [...document.querySelectorAll('button')].filter((button) => button.textContent.trim() === '查看详情').length === 27)
  assert.equal(await page.getByRole('button', { name: '查看详情', exact: true }).count(), 27)
  await page.getByRole('tab', { name: '企业认证', exact: true }).click()
  await page.getByLabel('认证主体名称', { exact: true }).fill('New Enterprise')
  await page.getByLabel('证件号码', { exact: true }).fill('91350211M000100Y43')
  await page.getByLabel('认证说明', { exact: true }).fill('Enterprise identity review')
  await page.locator('input[type="file"]').setInputFiles({ name: 'license.png', mimeType: 'image/png', buffer: png })
  await page.getByRole('button', { name: '提交认证', exact: true }).click()
  await page.getByText('New Enterprise', { exact: false }).first().waitFor()
  assert.equal(submitted, 1)
  await page.goto(`${home}?mode=admin`)
  await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
  await page.getByRole('button', { name: '通过认证', exact: true }).click()
  await page.locator('article').first().getByText('已通过', { exact: true }).waitFor()
  assert.equal(reviewed, 1)
  await page.goto(home)
  await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
  await page.getByRole('button', { name: '预览', exact: true }).click()
  await page.locator('img[alt="license.png"]').waitFor()
  assert.equal(downloads, 2, 'Approved materials must remain accessible')
  await page.goto(`${home}?mode=settings`)
  await page.getByText('实名认证配置', { exact: true }).waitFor()
  await capturePage('verification-settings')
  // Classic exposes explicit accessible names; HeroUI derives the switch names from labels.
  const toggle = async (label, checked) => {
    const named = page.getByRole('switch', { name: label, exact: true })
    const control = await named.count() ? named : label === '个人认证支付宝'
      ? page.getByRole('switch', { name: '启用支付宝实名认证', exact: true })
      : page.getByRole('switch', { name: '启用人工审核', exact: true }).nth(label === '个人认证人工审核' ? 0 : 1)
    if ((await control.evaluate((element) => element instanceof HTMLInputElement ? element.checked : element.getAttribute('aria-checked') === 'true')) !== checked) await control.click()
  }
  await toggle('个人认证人工审核', false)
  await toggle('企业认证人工审核', false)
  await toggle('个人认证支付宝', true)
  await page.getByRole('button', { name: '保存配置', exact: true }).click()
  await page.getByText('配置已保存并生效。', { exact: true }).waitFor()
  assert.equal(settingsWrites, 1)
  await page.goto(home)
  await page.waitForFunction(() => document.querySelector('select[aria-label="认证方式"]')?.value === 'alipay')
  assert.deepEqual(await providerSelect.locator('option').evaluateAll((options) => options.map((option) => option.value)), ['alipay'])
  assert.equal(await documentSelect.inputValue(), 'national_id')
  assert.equal(await documentSelect.isDisabled(), true)
  assert.equal(await page.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'CN')
  assert.equal(await page.locator('input[type="file"]').count(), 0)
  await page.getByRole('tab', { name: '企业认证', exact: true }).click()
  assert.equal(await page.getByRole('button', { name: '提交认证', exact: true }).isDisabled(), true)
  providers = []
  settings = { ...settings, enabled: false }
  await page.reload()
  await page.getByText('当前没有可用的认证方式，请联系管理员。', { exact: true }).waitFor()
  assert.equal(await page.getByRole('button', { name: '提交认证', exact: true }).isDisabled(), true)
  providers = ['manual', 'alipay']
  settings = { ...settings, individual_manual_enabled: true, enterprise_manual_enabled: true, enabled: true }
  await page.reload()
  await providerSelect.selectOption('manual')
  await documentSelect.selectOption('passport')
  await page.getByLabel('证件签发国家或地区代码', { exact: true }).fill('US')
  await page.locator('input[type="file"]').setInputFiles({ name: 'identity.png', mimeType: 'image/png', buffer: png })
  await providerSelect.selectOption('alipay')
  assert.equal(await page.locator('input[type="file"]').count(), 0)
  assert.equal(await page.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'CN')
  await providerSelect.selectOption('manual')
  assert.equal(await page.locator('input[type="file"]').evaluate((input) => input.files.length), 0)

  await page.goto(`${home}?mode=catalog`)
  await page.locator('article p').getByText('OpenAI', { exact: true }).waitFor()
  assert.equal(await page.locator('article').count(), 12)
  await capturePage('provider-catalog')
  const firstKeys = await page.locator('article p.font-mono').allTextContents()
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  assert.notDeepEqual(await page.locator('article p.font-mono').allTextContents(), firstKeys)
  await page.getByRole('textbox', { name: '搜索厂商', exact: true }).fill('deepseek')
  assert.equal(await page.locator('article').count(), 1)
  assert.equal(await page.locator('article > span svg').count(), 1, 'Provider cards must render the SVG brand mark')
  await page.getByRole('button', { name: '新增厂商', exact: true }).click()
  await page.getByPlaceholder('my-provider', { exact: true }).fill('example-provider')
  await page.getByPlaceholder('My Provider', { exact: true }).fill('Example Provider')
  await page.getByRole('button', { name: '保存', exact: true }).click()
  await page.getByText('厂商已保存。', { exact: true }).waitFor()
  assert.equal(catalogWrites, 1)
  await page.getByRole('textbox', { name: '搜索厂商', exact: true }).fill('example-provider')
  await page.getByText('Example Provider', { exact: true }).waitFor()
  await page.goto(`${home}?mode=admin`)
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
  console.log(`Verification browser checks passed for ${frontend}: cursor pagination, deferred previews, multipart upload, approval, retained materials, eligibility and provider catalog.`)
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
