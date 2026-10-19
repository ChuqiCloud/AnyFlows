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
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Browser checks run in remote CI')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const directory = resolve(root, frontend)
const html = resolve(directory, '.client-profiles-smoke.html')
const entry = resolve(directory, '.client-profiles-smoke.tsx')
const port = frontend === 'web' ? 4173 : 4174
const isNext = frontend === 'web-next'
let server
let browser
let page
let serverOutput = ''
const errors = []

try {
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Client profiles CI</title><div id="root"></div><script type="module" src="/.client-profiles-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React, { useState } from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { ClientProfilePage } from './src/features/client-profiles/client-profile-page';
    import { ChannelProfileDrawer } from './src/features/client-profiles/channel-profile-drawer';
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
    Object.assign(window, { profileCache: cache });
    function App() {
      const [channel, setChannel] = useState(false);
      return <main style={{ padding: 16, maxWidth: 1280, margin: 'auto' }}>
        <button onClick={() => setChannel(true)}>打开测试渠道</button>
        <ClientProfilePage />
        {channel && <ChannelProfileDrawer channel={{ id: 7, name: 'OpenCode CI', type: 'opencode', protocol: 'openai_chat' }} onClose={() => setChannel(false)} />}
      </main>;
    }
    createRoot(document.getElementById('root')!).render(<QueryClientProvider client={cache}>${isNext ? '<HeroUIProvider>' : ''}<App />${isNext ? '</HeroUIProvider>' : ''}</QueryClientProvider>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.client-profiles-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, `Vite failed to become ready: ${serverOutput}`)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(12_000)
  page.setDefaultNavigationTimeout(45_000)
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh') })

  const definition = { name: 'OpenCode Custom', description: 'CI profile', icon: 'OpenCode', service: 'open_code', protocols: ['openai_chat'], credential_kinds: ['api_key'], identity_headers: { 'user-agent': 'opencode/2.0.0' }, session_strategy: 'explicit', behavior: 'open_code_v1' }
  const standard = { ...definition, name: 'Standard', service: 'standard', behavior: 'standard', protocols: ['openai_chat', 'openai_responses', 'anthropic', 'gemini'], identity_headers: {} }
  const record = { key: 'opencode-custom', name: definition.name, service: 'open_code', built_in: false, enabled: true, latest_revision: 2, version: 4 }
  const records = [record, ...Array.from({ length: 25 }, (_, i) => ({ ...record, key: `standard-${i}`, name: `Standard ${i}`, service: 'standard', built_in: true }))]
  const snapshot = revision => ({ binding: { key: record.key, revision }, definition: { ...definition, protocols: revision === 1 ? ['anthropic'] : ['openai_chat'] } })
  const capabilities = { templates: [standard, definition], identity_headers: ['user-agent', 'x-app'], session_strategies: ['explicit', 'first_user_message'], opencode_entries: ['inference_v1', 'zen_v1'] }
  let binding = { channel_id: 7, binding: { key: 'standard-0', revision: 1 }, version: 1 }
  let settings = { entry: 'inference_v1', full_compatibility: false, version: 'entry-version-1' }
  let detailFailure = 0
  let write
  let savedBinding
  let savedEntry
  const lists = []
  let release
  let pending = new Promise(resolve => { release = resolve })
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname.replace('/api/admin/client-profiles', '')
    let body
    let status = 200
    if (path === '/capabilities') body = capabilities
    else if (path === '') {
      lists.push(Object.fromEntries(url.searchParams))
      if (pending) await pending
      assert.equal(url.searchParams.get('limit'), '25')
      const filtered = records.filter(item => !url.searchParams.get('search') || item.name.toLowerCase().includes(url.searchParams.get('search').toLowerCase()))
      const offset = url.searchParams.has('after') ? 25 : 0
      body = { items: filtered.slice(offset, offset + 25), next_cursor: filtered.length > offset + 25 ? 'page-2' : null }
    } else if (path === '/validate') body = { valid: true }
    else if (path === '/opencode-settings/7') {
      if (request.method() === 'PUT') { savedEntry = request.postDataJSON(); settings = { entry: savedEntry.entry, full_compatibility: savedEntry.full_compatibility, version: 'entry-version-2' } }
      body = settings
    } else if (path === '/channel-bindings/7') {
      if (request.method() === 'PUT') { savedBinding = request.postDataJSON(); binding = { ...binding, binding: savedBinding.binding, version: binding.version + 1 } }
      body = binding
    } else if (path.endsWith('/revisions/2')) body = snapshot(2)
    else if (path.endsWith('/revisions')) body = { items: [snapshot(2), snapshot(1)], next_cursor: null }
    else if (path.endsWith('/references')) body = { items: [binding], next_cursor: null }
    else if (request.method() === 'PUT') { write = request.postDataJSON(); body = { ...record, key: path.slice(1) } }
    else if (path === '/opencode-custom') { status = detailFailure || 200; body = status === 200 ? record : { code: 'profile_unavailable' } }
    else throw new Error(`Unexpected API request: ${request.method()} ${url.pathname}`)
    await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) })
  })
  const url = `http://127.0.0.1:${port}/.client-profiles-smoke.html`
  await page.goto(url)
  await page.getByRole('status', { name: '正在加载客户端档案…' }).waitFor()
  release(); pending = undefined
  await page.locator('article.cp-row').nth(24).waitFor()
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  await page.locator('article.cp-row').filter({ hasText: 'Standard 24' }).waitFor()
  await page.getByRole('textbox', { name: '搜索名称或标识', exact: true }).fill('OpenCode')
  await page.locator('article.cp-row').filter({ hasText: 'OpenCode Custom' }).waitFor()
  assert.equal(lists.at(-1).after, undefined, 'New search must reset the cursor')
  assert.equal(await page.locator('article.cp-row').count(), 1)
  await page.getByRole('button', { name: '复制', exact: true }).click()
  const field = name => page.getByRole('textbox', { name, exact: true })
  await field('档案标识').fill('copy-ci')
  await field('显示名称').fill('Draft retained')
  detailFailure = 503
  await page.evaluate(() => window.profileCache.invalidateQueries({ queryKey: ['client-profiles', 'detail', 'opencode-custom'] }))
  await page.getByRole('alert').waitFor()
  assert.equal(await field('显示名称').inputValue(), 'Draft retained', 'Background errors must preserve the draft')
  detailFailure = 0
  await page.getByRole('button', { name: '重新加载', exact: true }).click()
  await page.getByRole('alert').waitFor({ state: 'hidden' })
  for (const width of [1280, 390, 320]) {
    await page.setViewportSize({ width, height: 960 })
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${frontend} drawer overflows at ${width}px`)
    if (isNext) {
      const select = page.getByRole('button', { name: '会话关联', exact: true })
      await select.scrollIntoViewIfNeeded()
      await select.click()
      await page.getByRole('option', { name: '明确标识优先，首条消息尽力关联', exact: true }).click()
      await page.getByRole('listbox').waitFor({ state: 'hidden' })
      assert.ok((await select.innerText()).includes('首条消息'), 'The selected strategy must remain visible')
    } else await page.getByRole('combobox', { name: '会话关联', exact: true }).selectOption('first_user_message')
  }
  await page.keyboard.press('Escape')
  await page.getByText('有尚未保存的修改，是否放弃？', { exact: true }).waitFor()
  await page.getByRole('button', { name: '继续编辑', exact: true }).click()
  assert.equal(await field('显示名称').inputValue(), 'Draft retained')
  await page.getByRole('button', { name: '校验配置', exact: true }).click()
  await page.getByText('配置校验通过。渠道绑定时仍会检查协议及请求头冲突。', { exact: true }).waitFor()
  await page.getByRole('button', { name: '创建档案', exact: true }).click()
  await field('档案标识').waitFor({ state: 'hidden' })
  assert.equal(write.expected_version, 0)
  assert.equal(write.definition.session_strategy, 'first_user_message')
  assert.equal(write.definition.name, 'Draft retained')

  await page.getByRole('button', { name: '打开测试渠道', exact: true }).click()
  const drawer = page.locator('.cp-drawer-body')
  await drawer.locator('.cp-row').first().waitFor()
  assert.ok(await drawer.locator('.cp-table').evaluate(element => element.getBoundingClientRect().height >= element.querySelectorAll('.cp-row').length * 32), 'The drawer must not collapse a populated profile list')
  await drawer.getByRole('button', { name: '选择', exact: true }).first().click()
  const oldRevision = drawer.locator('.cp-version').filter({ hasText: 'r1 · OpenCode Custom' })
  await oldRevision.waitFor()
  assert.ok(await oldRevision.getByRole('button', { name: '选择', exact: true }).isDisabled())
  await oldRevision.getByText('此修订的服务或协议与当前渠道不兼容，无法选择。', { exact: true }).waitFor()
  if (isNext) {
    const endpoint = page.getByRole('button', { name: '服务入口', exact: true })
    await endpoint.scrollIntoViewIfNeeded()
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
    await endpoint.click()
    assert.equal(await endpoint.getAttribute('aria-expanded'), 'true', 'Endpoint menu should remain open after pointer input')
    await page.getByRole('option', { name: 'Zen v1', exact: true }).click()
  } else await page.getByRole('combobox', { name: '服务入口', exact: true }).selectOption('zen_v1')
  const compatibility = page.getByRole('switch', { name: '完整工具兼容', exact: true })
  assert.equal(await compatibility.isChecked(), false)
  await compatibility.check()
  await page.getByRole('button', { name: '保存服务入口', exact: true }).click()
  await page.getByText('配置已保存。', { exact: true }).waitFor()
  assert.deepEqual(savedEntry, { entry: 'zen_v1', full_compatibility: true, version: 'entry-version-1' })
  assert.equal(savedBinding, undefined, 'Changing the endpoint must not overwrite the binding')
  assert.equal(await compatibility.isChecked(), true)
  await page.getByRole('button', { name: '切换到此修订', exact: true }).click()
  await page.getByRole('button', { name: '切换到此修订', exact: true }).waitFor({ state: 'hidden' })
  assert.deepEqual(savedBinding, { binding: { key: record.key, revision: 2 }, expected_version: 1 })
  await page.waitForFunction(() => [...document.querySelectorAll('.cp-drawer-body button')].some(button => button.textContent.trim() === '已选择' && !button.disabled))
  const screenshot = await page.screenshot({ fullPage: true, animations: 'disabled' })
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_client_profiles_BEGIN`)
  console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_client_profiles_END`)
  await page.keyboard.press('Escape')
  await drawer.waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: '发布修订', exact: true }).click()
  await field('显示名称').waitFor()
  detailFailure = 403
  await page.evaluate(() => window.profileCache.invalidateQueries({ queryKey: ['client-profiles', 'detail', 'opencode-custom'] }))
  await page.getByText('当前账号没有管理客户端档案的权限。', { exact: true }).waitFor()
  assert.equal(await field('显示名称').count(), 0, 'Access denial must hide the old definition')
  assert.deepEqual(errors, [], 'Client profiles must not raise browser errors')
  console.log(`Client profile browser checks passed for ${frontend}: loading, cursor isolation, frozen drafts, save payload, mobile selects, discard protection, revision compatibility, independent endpoint saves and permission errors.`)
} catch (error) {
  if (page) {
    console.error((await page.locator('body').ariaSnapshot()).slice(-16_000))
    const screenshot = await page.screenshot({ fullPage: true, animations: 'disabled' })
    console.log(`ANYFLOWS_SCREENSHOT_${frontend}_client_profiles_failure_BEGIN`)
    console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
    console.log(`ANYFLOWS_SCREENSHOT_${frontend}_client_profiles_failure_END`)
  }
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
