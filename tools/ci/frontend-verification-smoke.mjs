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
    import { SoftwareUpdatePanel } from './src/features/site-settings/software-update-panel';
    import { AuthenticationSettingsPage } from './src/features/authentication-settings/authentication-settings-page';
    import { resolveProviderLogo, findProviderLogo } from './src/components/brand/model-logos';
    if (resolveProviderLogo('Jina', 'deepseek')?.name !== 'Jina') throw new Error('Configured logo must take priority');
    if (resolveProviderLogo(null, 'private-provider', 'DeepSeek')?.name !== 'DeepSeek') throw new Error('Display name must identify logo');
    if (findProviderLogo('google-vertex')?.name !== 'Google Vertex AI') throw new Error('Compound aliases must identify logo');
    const mode = new URLSearchParams(location.search).get('mode');
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '16px', maxWidth: '1100px', margin: 'auto' }}>
          {mode === 'software' ? <SoftwareUpdatePanel /> : mode === 'rewards' ? <AuthenticationSettingsPage rewardsOnly /> : mode === 'settings' ? <VerificationSettingsPanel /> : mode === 'catalog' ? <ModelProviderCatalogPanel /> : <AccountVerificationPage admin={mode === 'admin'} />}
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
  let releaseVerification
  const verificationReady = new Promise((resolve) => { releaseVerification = resolve })
  let submissionKind = 'enterprise'
  let submitted = 0
  let allowVerificationSubmission = false
  let unexpectedVerificationSubmissions = 0
  let reviewed = 0
  let downloads = 0
  let providers = ['manual']
  let settings = { source: 'database', manual_enabled: true, individual_manual_enabled: true, enterprise_manual_enabled: true, individual_reason_required: true, enterprise_reason_required: true, enabled: false, app_id: 'test-app', private_key_configured: true, public_key_configured: true, gateway_url: 'https://openapi.alipay.com/gateway.do', biz_code: 'FACE', timeout_secs: 10, version: 1 }
  let settingsWrites = 0
  const catalog = [{ provider_key: 'custom-ai', display_name: 'Custom AI', logo: 'OpenAI', aliases: [], enabled: true, sort_order: 0, version: 1 }]
  let catalogWrites = 0
  let rewardWrites = 0
  let rewardSettings = { password_login_enabled: true, registration_enabled: true, default_group_id: 3, initial_quota: 100, invitation_rebate_quota: 20, invitation_invitee_quota: 50, email_required: true, rate_limit_attempts: 7, rate_limit_window_seconds: 900, version: 4 }
  const cases = Array.from({ length: 27 }, (_, index) => ({ id: 27 - index, user_id: 1, kind: 'enterprise', provider: 'manual', document_country: 'CN', document_type: 'business_registration', document_number_masked: '91****43', subject_name: `Enterprise ${27 - index}`, summary: 'Historical application', status: 5, version: 2, review_reason: 'Supplement required', reviewer_user_id: 2, created_at: 1_780_000_000, updated_at: 1_780_000_010 }))
  await page.route((url) => url.pathname.startsWith('/api/'), async (route) => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname
    let body
    if (path === '/api/extensions') body = []
    else if (path === '/api/admin/authentication-settings') {
      if (request.method() === 'PUT') {
        const update = request.postDataJSON()
        const expected = { ...rewardSettings, initial_quota: 200, invitation_rebate_quota: 30, invitation_invitee_quota: 75 }
        delete expected.version
        assert.deepEqual(update, expected)
        rewardSettings = { ...rewardSettings, ...update, version: rewardSettings.version + 1 }
        rewardWrites += 1
      }
      body = rewardSettings
    } else if (path === '/api/admin/groups') body = { groups: [{ id: 3, name: 'default', display_name: '默认分组', enabled: true, version: 1 }], next_cursor: null }
    else if (path.endsWith('/eligibility')) body = { enterprise_verified: reviewed > 0, can_apply_for_organization: reviewed > 0, providers, individual_providers: [...(settings.individual_manual_enabled ? ['manual'] : []), ...(settings.enabled ? ['alipay'] : [])], enterprise_providers: settings.enterprise_manual_enabled ? ['manual'] : [], individual_reason_required: settings.individual_reason_required, enterprise_reason_required: settings.enterprise_reason_required }
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
      const recordId = Number(path.split('/').at(-2))
      const recordIndex = cases.findIndex((item) => item.id === recordId)
      assert.notEqual(recordIndex, -1, `Cannot review missing verification case ${recordId}`)
      assert.equal(update.expected_version, cases[recordIndex].version)
      assert.equal(update.status, 4)
      cases[recordIndex] = { ...cases[recordIndex], status: 4, version: cases[recordIndex].version + 1 }
      reviewed += 1
      body = cases[recordIndex]
    } else if (path === '/api/account/verifications' && request.method() === 'POST') {
      if (!allowVerificationSubmission) {
        unexpectedVerificationSubmissions += 1
        await route.fulfill({ status: 409, contentType: 'application/json', body: JSON.stringify({ error: 'Unexpected verification submission' }) })
        return
      }
      allowVerificationSubmission = false
      const contentType = request.headers()['content-type']
      const postData = request.postDataBuffer()
      const postBody = postData?.toString('latin1') ?? ''
      const multipartFields = [...postBody.matchAll(/Content-Disposition:[^\r\n]*?name="([^"]+)"/giu)].map((match) => match[1])
      const metadataHeader = postBody.indexOf('name="metadata"')
      const metadataStart = postBody.indexOf('\r\n\r\n', metadataHeader)
      const metadataPart = metadataStart < 0 ? undefined : postBody.slice(metadataStart + 4).split('\r\n', 1)[0]
      const metadata = metadataPart ? JSON.parse(metadataPart) : undefined
      const metadataSummary = metadata && {
        kind: metadata.kind,
        provider: metadata.provider,
        document_country: metadata.document_country,
        document_type: metadata.document_type,
        materials: metadata.materials,
      }
      const fileFields = multipartFields.filter((field) => /^file:\d+$/u.test(field)).sort()
      assert.match(contentType, /^multipart\/form-data; boundary=/, `${path} must use multipart form data`)
      assert.ok(['manual', 'alipay'].includes(metadata?.provider), `${path} metadata had an unexpected provider: ${JSON.stringify(metadataSummary)}`)
      assert.deepEqual(fileFields, (metadata?.materials ?? []).map((material) => `file:${material.file_field}`).sort(), `${path} file fields did not match the declared materials: ${JSON.stringify(metadataSummary)}`)
      if (metadata.provider === 'manual') assert.ok(fileFields.includes('file:0'), `${path} manual review must upload its first material: ${JSON.stringify(metadataSummary)}`)
      else assert.deepEqual(metadata.materials, [], `${path} Alipay verification must not upload manual-review materials: ${JSON.stringify(metadataSummary)}`)
      assert.equal(metadata?.kind, submissionKind, `${path} metadata had an unexpected kind: ${JSON.stringify(metadataSummary)}`)
      assert.equal(metadata?.document_country, 'CN', `${path} metadata had an unexpected document country: ${JSON.stringify(metadataSummary)}`)
      const expectedDocumentType = metadata.provider === 'alipay' ? 'national_id' : submissionKind === 'enterprise' ? 'business_registration' : 'passport'
      assert.equal(metadata?.document_type, expectedDocumentType, `${path} metadata had an unexpected document type: ${JSON.stringify(metadataSummary)}`)
      cases.unshift({ ...cases[0], id: 28 + submitted, kind: submissionKind, provider: metadata.provider, subject_name: metadata.subject_name, document_type: metadata.document_type, document_number_masked: submissionKind === 'enterprise' ? '91****43' : 'P****78', status: 1, version: 1, review_reason: null, reviewer_user_id: null })
      submitted += 1
      body = cases[0]
    } else if (/\/\d+$/.test(path)) {
      const record = cases.find((item) => item.id === Number(path.split('/').at(-1)))
      assert.ok(record)
      body = { case: record, materials: [{ id: 1, case_id: record.id, kind: 'business_document', file_name: 'license.png', content_type: 'image/png', size_bytes: png.length }] }
    } else {
      await verificationReady
      assert.ok(['/api/account/verifications', '/api/admin/account-verifications'].includes(path), path)
      const before = Number(url.searchParams.get('before') ?? Infinity)
      const status = url.searchParams.get('status')
      const rows = cases.filter((item) => item.id < before && (!status || item.status === Number(status))).slice(0, 25)
      body = { cases: rows, next_cursor: rows.length === 25 ? rows.at(-1).id : null }
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) })
  })
  const home = `http://127.0.0.1:${port}/.verification-smoke.html`
  const submitVerification = async (scope = page) => {
    allowVerificationSubmission = true
    const response = page.waitForResponse((value) => {
      const request = value.request()
      return request.method() === 'POST' && new URL(value.url()).pathname === '/api/account/verifications'
    })
    await scope.getByRole('button', { name: '提交认证', exact: true }).click()
    await response
  }
  await page.goto(home)
  await page.getByRole('status').first().waitFor()
  releaseVerification()
  await page.locator('#verification-case-27').waitFor()
  assert.equal(await page.getByText('Enterprise 27', { exact: false }).count(), 0, 'History must mask identity names')
  if (isNext) {
    // web-next uses type cards, provider buttons and a three-step form.
    // Exercise the public controls instead of Classic's native selects and tabs.
    const historyRows = page.locator('li[id^="verification-case-"]')
    const form = page.locator('#verification-form')
    const selectTrigger = (name, scope = page) => scope.locator(`button[data-slot="trigger"][aria-label=${JSON.stringify(name)}]`).filter({ visible: true })
    const assertSelectDisabled = async (name) => {
      // HeroUI Select exposes disabled state on its trigger and its own hidden native select.
      const trigger = selectTrigger(name, form)
      const triggerMarkup = await trigger.evaluateAll((elements) => elements.map((element) => element.outerHTML))
      assert.equal(triggerMarkup.length, 1, `Expected one visible ${name} trigger, got ${JSON.stringify(triggerMarkup)}`)
      assert.equal(await trigger.getAttribute('data-disabled'), 'true', `HeroUI trigger must report disabled: ${triggerMarkup[0]}`)
      assert.notEqual(await trigger.getAttribute('data-open'), 'true', `Disabled ${name} trigger must start closed: ${triggerMarkup[0]}`)
      const nativeSelect = trigger.locator('xpath=ancestor::*[@data-slot="base"][1]').locator('select')
      assert.equal(await nativeSelect.count(), 1, `Expected one native select associated with ${name}: ${triggerMarkup[0]}`)
      assert.equal(await nativeSelect.isDisabled(), true, `Native select must be disabled with its HeroUI trigger: ${triggerMarkup[0]}`)
      await trigger.press('ArrowDown')
      assert.notEqual(await trigger.getAttribute('data-open'), 'true', `Disabled ${name} trigger must not open its menu: ${triggerMarkup[0]}`)
      assert.equal(await page.getByRole('listbox').count(), 0, `Disabled ${name} trigger must not expose a listbox: ${triggerMarkup[0]}`)
    }
    const chooseOption = async (name, option) => {
      await selectTrigger(name).click()
      await page.getByRole('option', { name: option, exact: true }).click()
    }
    const provider = (label) => form.locator('button[aria-pressed]').filter({ hasText: label })
    const nextStep = () => form.getByRole('button', { name: '下一步', exact: true }).click()
    const previousStep = () => form.getByRole('button', { name: '返回', exact: true }).click()
    const chooseKind = async (name) => {
      await page.getByRole('button').filter({ has: page.getByRole('heading', { name, exact: true }) }).click()
      await form.waitFor()
    }
    const openRecord = async (id) => {
      const row = page.locator(`#verification-case-${id}`)
      await row.locator('button[aria-expanded="false"]').click()
      await row.getByText('license.png', { exact: true }).waitFor()
      return row
    }
    const fillIdentity = async (name, number, summary) => {
      await form.getByLabel('认证主体名称', { exact: true }).fill(name)
      await form.getByLabel('证件号码', { exact: true }).fill(number)
      await form.getByLabel('认证说明', { exact: true }).fill(summary)
    }
    const approve = async (id) => {
      await page.goto(`${home}?mode=admin`)
      await chooseOption('按审核状态筛选', '全部状态')
      const row = await openRecord(id)
      await row.getByRole('button', { name: '通过认证', exact: true }).click()
      await row.getByText('已通过', { exact: true }).first().waitFor()
    }
    const replace = async () => {
      const verifiedResponse = page.waitForResponse((response) => {
        const url = new URL(response.url())
        return url.pathname === '/api/account/verifications' && url.searchParams.get('status') === '4'
      })
      await page.goto(`${home}?fixture=${Date.now()}`)
      const verifiedPayload = await (await verifiedResponse).json()
      assert.ok(verifiedPayload.cases.some((item) => item.status === 4), `Expected an approved verification fixture; statuses=${cases.map((item) => `${item.id}:${item.status}`).join(',')}`)
      await page.locator('[data-verification-result]').waitFor()
      await page.getByRole('button', { name: '更换认证', exact: true }).click()
      await form.waitFor()
    }
    assert.equal(await historyRows.count(), 10)
    assert.equal(downloads, 0)
    const historical = await openRecord(27)
    assert.equal(downloads, 0, 'Opening record metadata must not download file contents')
    await historical.getByRole('button', { name: '预览', exact: true }).click()
    await historical.locator('img[alt="license.png"]').waitFor()
    assert.equal(downloads, 1)
    await page.getByRole('button', { name: '下一页', exact: true }).click()
    await page.locator('#verification-case-17').waitFor()
    const cursorPage = page.waitForResponse((response) => new URL(response.url()).pathname === '/api/account/verifications' && new URL(response.url()).searchParams.get('before') === '3')
    await page.getByRole('button', { name: '下一页', exact: true }).click()
    await cursorPage
    await page.locator('#verification-case-1').waitFor()
    assert.equal(await historyRows.count(), 7, 'The partial final page must include the next cursor page')
    assert.equal(await page.getByRole('button', { name: '下一页', exact: true }).isDisabled(), true)
    await chooseOption('每页', '50')
    await page.locator('#verification-case-27').waitFor()
    assert.equal(await historyRows.count(), 27)

    await chooseKind('企业认证')
    await provider('人工审核').waitFor()
    assert.equal(await form.locator('button[aria-pressed]').count(), 1)
    await nextStep()
    await fillIdentity('New Enterprise', '91350211M000100Y43', 'Enterprise identity review')
    await form.getByLabel('证件签发国家或地区代码', { exact: true }).fill('BR')
    assert.equal(await form.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'BR')
    await form.getByLabel('证件签发国家或地区代码', { exact: true }).fill('CN')
    await assertSelectDisabled('证件类型')
    await nextStep()
    await form.locator('input[type="file"]').setInputFiles({ name: 'license.png', mimeType: 'image/png', buffer: png })
    await submitVerification()
    await page.locator('#verification-case-28').waitFor()
    assert.equal(submitted, 1)
    await approve(28)
    assert.equal(reviewed, 1)

    await page.goto(home)
    const verifiedResult = page.locator('[data-verification-result]')
    await verifiedResult.waitFor()
    assert.equal(await verifiedResult.getByText('New Enterprise', { exact: true }).count(), 0)
    await verifiedResult.getByText('91****43', { exact: true }).waitFor()
    assert.equal(await form.count(), 0)
    const approved = await openRecord(28)
    await approved.getByRole('button', { name: '预览', exact: true }).click()
    await approved.locator('img[alt="license.png"]').waitFor()
    assert.equal(downloads, 2, 'Approved materials must remain accessible')
    await capturePage('verified-account')
    await page.evaluate(() => {
      window.verificationScrolls = 0
      const scroll = Element.prototype.scrollIntoView
      Element.prototype.scrollIntoView = function (...args) { window.verificationScrolls += 1; return scroll.apply(this, args) }
    })
    const refreshed = page.waitForResponse((response) => new URL(response.url()).pathname === '/api/account/verifications' && !new URL(response.url()).searchParams.has('status'))
    await page.getByRole('button', { name: '刷新', exact: true }).click()
    await refreshed
    assert.equal(await page.evaluate(() => window.verificationScrolls), 0)
    await page.getByRole('button', { name: '更换认证', exact: true }).click()
    await page.getByText('新资料审核期间，原认证结果与已有资格继续有效。', { exact: true }).waitFor()
    submissionKind = 'individual'
    await provider('人工审核').click()
    await nextStep()
    await chooseOption('证件类型', '护照')
    await fillIdentity('Test Individual', 'P12345678', 'Personal manual review')
    await nextStep()
    await form.locator('input[type="file"]').setInputFiles({ name: 'passport.png', mimeType: 'image/png', buffer: png })
    await submitVerification()
    await page.locator('#verification-case-29').waitFor()
    assert.equal(submitted, 2)
    await approve(29)
    assert.equal(reviewed, 2)
    assert.equal(await page.locator('input[type="file"]').count(), 0)
    await capturePage('account-verification-reviews')

    await page.goto(`${home}?mode=settings`)
    await page.getByText('实名认证配置', { exact: true }).waitFor()
    await capturePage('verification-settings')
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
    await replace()
    await provider('支付宝实名认证').waitFor()
    assert.equal(await form.locator('button[aria-pressed]').count(), 1)
    await nextStep()
    await assertSelectDisabled('证件类型')
    assert.match(await selectTrigger('证件类型').textContent(), /身份证/)
    assert.equal(await form.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'CN')
    await fillIdentity('Alipay Applicant', '11010519491231002X', 'Alipay identity review')
    await nextStep()
    assert.equal(await form.locator('input[type="file"]').count(), 0)
    assert.equal(submitted, 2, 'Alipay identity completion must not submit before the explicit action')
    assert.equal(unexpectedVerificationSubmissions, 0, 'Advancing form steps must not issue verification requests')

    providers = []
    settings = { ...settings, enabled: false }
    const savedCases = cases.map((item) => ({ ...item }))
    cases.forEach((item) => { item.status = 5 })
    await page.goto(home)
    await chooseKind('个人认证')
    await form.getByText('当前没有可用的认证方式，请联系管理员。', { exact: true }).waitFor()
    assert.equal(await form.getByRole('button', { name: '下一步', exact: true }).isDisabled(), true)
    await page.goto(home)
    await chooseKind('企业认证')
    await form.getByText('当前没有可用的认证方式，请联系管理员。', { exact: true }).waitFor()
    assert.equal(await form.getByRole('button', { name: '下一步', exact: true }).isDisabled(), true)
    cases.splice(0, cases.length, ...savedCases)

    providers = ['manual', 'alipay']
    settings = { ...settings, individual_manual_enabled: true, enterprise_manual_enabled: true, enabled: true }
    const originalStates = cases.map((item) => ({ ...item }))
    cases.forEach((item) => { item.status = 5 })
    cases.unshift({ ...cases[0], id: 30, status: 3, provider: 'manual', provider_status: null })
    try {
      await page.goto(home)
      await page.getByRole('button', { name: '补充材料', exact: true }).click()
      await form.waitFor()
      await page.getByText('新资料审核期间，原认证结果与已有资格继续有效。', { exact: true }).waitFor()
      await page.getByRole('button', { name: '返回认证结果', exact: true }).click()
      await form.waitFor({ state: 'detached' })
      await page.getByRole('button', { name: '补充材料', exact: true }).waitFor()
    } finally {
      cases.splice(0, cases.length, ...originalStates)
    }

    cases.forEach((item) => { item.status = 5 })
    cases.unshift({ ...cases[0], id: 30, status: 1, provider: 'alipay', provider_status: 'expired', kind: 'individual' })
    try {
      await page.goto(home)
      const expiredRecord = await openRecord(30)
      await expiredRecord.getByRole('button', { name: '重新填写认证', exact: true }).click()
      await form.waitFor()
      await provider('支付宝实名认证').waitFor()
      assert.equal(await provider('支付宝实名认证').getAttribute('aria-pressed'), 'true', 'Expired Alipay authorization must reopen with Alipay selected')
    } finally {
      cases.splice(0, cases.length, ...originalStates)
    }

    await page.goto(`${home}?fixture=${Date.now()}`)
    await chooseKind('个人认证')
    await provider('人工审核').click()
    await nextStep()
    await chooseOption('证件类型', '护照')
    await fillIdentity('Provider Switch', 'P12345678', 'Switch provider without retained materials')
    await form.getByLabel('证件签发国家或地区代码', { exact: true }).fill('US')
    await nextStep()
    await form.locator('input[type="file"]').setInputFiles({ name: 'identity.png', mimeType: 'image/png', buffer: png })
    await previousStep()
    await previousStep()
    await provider('支付宝实名认证').click()
    await nextStep()
    assert.equal(await form.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'CN')
    await assertSelectDisabled('证件类型')
    await previousStep()
    await provider('人工审核').click()
    await nextStep()
    await form.getByLabel('证件号码', { exact: true }).fill('P87654321')
    await nextStep()
    assert.equal(await form.locator('input[type="file"]').evaluate((input) => input.files.length), 0)
  } else {
    const providerSelect = page.getByRole('combobox', { name: '认证方式', exact: true })
    const documentSelect = page.getByRole('combobox', { name: '证件类型', exact: true })
    await providerSelect.locator('option[value="manual"]').waitFor({ state: 'attached' })
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
    await submitVerification()
    await page.locator('#verification-case-28').waitFor()
    assert.equal(submitted, 1)
    await page.goto(`${home}?mode=admin`)
    await page.getByRole('combobox', { name: '按审核状态筛选', exact: true }).selectOption('')
    await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
    await page.getByRole('button', { name: '通过认证', exact: true }).click()
    await page.locator('article').first().getByText('已通过', { exact: true }).waitFor()
    assert.equal(reviewed, 1)
    await page.goto(home)
    await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
    await page.getByRole('button', { name: '预览', exact: true }).click()
    await page.locator('img[alt="license.png"]').waitFor()
    assert.equal(downloads, 2, 'Approved materials must remain accessible')
    await page.evaluate(() => {
      window.verificationScrolls = 0
      const scroll = Element.prototype.scrollIntoView
      Element.prototype.scrollIntoView = function (...args) { window.verificationScrolls += 1; return scroll.apply(this, args) }
    })
    const refreshed = page.waitForResponse((response) => new URL(response.url()).pathname === '/api/account/verifications' && !new URL(response.url()).searchParams.has('status'))
    await page.getByRole('button', { name: '刷新', exact: true }).click()
    await refreshed
    assert.equal(await page.evaluate(() => window.verificationScrolls), 0, 'Refreshing history must not repeatedly scroll a selected record')
    await page.goto(home)
    const verifiedResult = page.locator('[data-verification-result]')
    await verifiedResult.waitFor()
    assert.equal(await verifiedResult.getByText('New Enterprise', { exact: true }).count(), 0)
    await verifiedResult.getByText('91****43', { exact: true }).waitFor()
    assert.equal(await page.locator('#verification-form').count(), 0, 'Approved accounts should show the result first')
    await capturePage('verified-account')
    await page.getByRole('button', { name: '更换认证', exact: true }).click()
    await page.locator('#verification-form').waitFor()
    await page.getByText('新资料审核期间，原认证结果与已有资格继续有效。', { exact: true }).waitFor()
    submissionKind = 'individual'
    await page.getByRole('tab', { name: '个人认证', exact: true }).click()
    await page.getByLabel('认证主体名称', { exact: true }).fill('Test Individual')
    await page.getByRole('combobox', { name: '证件类型', exact: true }).selectOption('passport')
    await page.getByLabel('证件号码', { exact: true }).fill('P12345678')
    await page.getByLabel('认证说明', { exact: true }).fill('Personal manual review')
    await page.locator('input[type="file"]').setInputFiles({ name: 'passport.png', mimeType: 'image/png', buffer: png })
    await submitVerification()
    await page.locator('#verification-case-29').waitFor()
    assert.equal(submitted, 2)
    await page.goto(`${home}?mode=admin`)
    await page.getByRole('combobox', { name: '按审核状态筛选', exact: true }).selectOption('')
    assert.equal(await page.locator('input[type="file"]').count(), 0)
    await page.getByRole('button', { name: '查看详情', exact: true }).first().click()
    await page.getByRole('button', { name: '通过认证', exact: true }).click()
    await page.locator('article').first().getByText('已通过', { exact: true }).waitFor()
    assert.equal(reviewed, 2)
    await capturePage('account-verification-reviews')
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
    await page.getByRole('button', { name: '更换认证', exact: true }).click()
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
    await page.getByRole('button', { name: '更换认证', exact: true }).click()
    await page.getByText('当前没有可用的认证方式，请联系管理员。', { exact: true }).waitFor()
    assert.equal(await page.getByRole('button', { name: '提交认证', exact: true }).isDisabled(), true)
    providers = ['manual', 'alipay']
    settings = { ...settings, individual_manual_enabled: true, enterprise_manual_enabled: true, enabled: true }
    await page.reload()
    await page.getByRole('button', { name: '更换认证', exact: true }).click()
    await providerSelect.selectOption('manual')
    await documentSelect.selectOption('passport')
    await page.getByLabel('证件签发国家或地区代码', { exact: true }).fill('US')
    await page.locator('input[type="file"]').setInputFiles({ name: 'identity.png', mimeType: 'image/png', buffer: png })
    await providerSelect.selectOption('alipay')
    assert.equal(await page.locator('input[type="file"]').count(), 0)
    assert.equal(await page.getByLabel('证件签发国家或地区代码', { exact: true }).inputValue(), 'CN')
    await providerSelect.selectOption('manual')
    assert.equal(await page.locator('input[type="file"]').evaluate((input) => input.files.length), 0)

  }

  await page.goto(`${home}?mode=rewards`)
  await page.getByRole('heading', { name: '注册与邀请奖励', level: 2, exact: true }).waitFor()
  await page.locator('#authentication-initial-quota').waitFor()
  assert.equal(await page.getByRole('switch').count(), 0)
  await page.locator('#authentication-initial-quota').fill('200')
  await page.locator('#authentication-invitation-rebate-quota').fill('30')
  await page.locator('#authentication-invitation-invitee-quota').fill('75')
  await page.getByRole('button', { name: '保存设置', exact: true }).click()
  await page.getByText('奖励设置已保存', { exact: true }).waitFor()
  assert.equal(rewardWrites, 1)
  await capturePage('invitation-settings')

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
  let software = { edition: 'community', version: '0.3.0', core_version: '0.3.0', commit: 'a'.repeat(40), build_time: '2026-10-24T00:00:00Z', target: 'x86_64-unknown-linux-gnu', deployment: 'binary', node_id: 'node-fixture', launch_mode: 'direct', launcher_protocol: 1, launcher_version: null, update_phase: 'idle', launcher_error: null, check_enabled: true, install_enabled: true, install_blocked_reason: null, restart_required: false, checked_at: null, latest: null, update_available: false }
  let softwareChecks = 0
  let softwareInstalls = 0
  await page.route((url) => url.pathname.startsWith('/api/admin/software'), async (route) => {
    const path = new URL(route.request().url()).pathname
    if (path.endsWith('/check')) {
      softwareChecks += 1
      software = { ...software, checked_at: 1_792_800_000, update_available: true, latest: { version: '0.4.0', tag: 'v0.4.0', url: 'https://github.com/ChuqiCloud/AnyFlows/releases/tag/v0.4.0', notes: 'Update fixture <script>unsafe</script>', published_at: null, package_name: 'anyflows-v0.4.0-linux-amd64.tar.gz', package_url: 'https://github.com/ChuqiCloud/AnyFlows/releases/download/v0.4.0/anyflows-v0.4.0-linux-amd64.tar.gz' } }
    } else if (path.endsWith('/install')) {
      assert.deepEqual(route.request().postDataJSON(), { tag: 'v0.4.0', expected_node_id: 'node-fixture' })
      softwareInstalls += 1
      software = { ...software, restart_required: true, install_enabled: false, install_blocked_reason: 'restart_required' }
    }
    await route.fulfill({ json: software })
  })
  await page.goto(`${home}?mode=software`)
  const versionDetails = page.locator('details').filter({ has: page.getByText('node-fixture', { exact: true }) })
  await versionDetails.locator('summary').waitFor()
  assert.equal(await versionDetails.getByText('node-fixture', { exact: true }).isVisible(), false, 'Version details start collapsed')
  await versionDetails.locator('summary').click()
  await versionDetails.getByText('node-fixture', { exact: true }).waitFor()
  assert.equal(softwareChecks, 0, 'Opening version information must not contact release discovery')
  await page.getByRole('button', { name: '检查更新', exact: true }).click()
  const installUpdate = page.getByRole('button', { name: '立即更新', exact: true })
  await installUpdate.waitFor()
  assert.equal(softwareChecks, 1)
  assert.ok(await installUpdate.isDisabled(), 'Install requires explicit backup acknowledgement')
  await page.getByRole('checkbox').check()
  await installUpdate.click()
  await page.getByText('安装完成，重启后生效。旧程序保留在程序目录的 anyflows-backup-* 文件中。', { exact: true }).waitFor()
  assert.equal(softwareInstalls, 1)
  assert.equal(await installUpdate.count(), 0, 'Installed process cannot install again before restarting')
  software = { ...software, deployment: 'docker', restart_required: false, install_blocked_reason: 'container' }
  await page.reload()
  await page.getByText('容器部署仅支持检查版本，请使用新的镜像标签重新部署。', { exact: true }).waitFor()
  assert.equal(await installUpdate.count(), 0)
  assert.equal(await page.getByRole('checkbox').count(), 0)
  software = { ...software, edition: 'custom', deployment: 'binary', check_enabled: false, install_blocked_reason: 'host_managed' }
  await page.reload()
  await page.getByText('当前部署暂不开放在线更新。', { exact: true }).waitFor()
  assert.equal(await page.getByRole('button', { name: '检查更新', exact: true }).count(), 0)
  for (const width of [1280, 390]) {
    await page.setViewportSize({ width, height: 844 })
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${frontend} version panel overflowed at ${width}px`)
  }
  const softwareScreenshot = await page.screenshot({ fullPage: true, animations: 'disabled' })
  console.log(`ANYFLOWS_SOFTWARE_SCREENSHOT_${frontend}_BEGIN`)
  console.log(softwareScreenshot.toString('base64').match(/.{1,120}/g).join('\n'))
  console.log(`ANYFLOWS_SOFTWARE_SCREENSHOT_${frontend}_END`)
  console.log(`Software update browser checks passed for ${frontend}: explicit discovery, install acknowledgement, node binding, restart, container and managed-host policies.`)
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
