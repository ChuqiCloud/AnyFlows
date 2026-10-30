import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdir, unlink, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

// This fixture and browser server run only on the remote frontend CI runner.
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const frontend = process.argv[2]
assert.ok(['web', 'web-next'].includes(frontend))
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Browser checks run in remote CI')
assert.ok(process.env.RUNNER_TEMP, 'Screenshots stay on the remote CI runner')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const directory = resolve(root, frontend)
const isNext = frontend === 'web-next'
const port = isNext ? 4174 : 4173
const html = resolve(directory, '.about-smoke.html')
const entry = resolve(directory, '.about-smoke.tsx')
const screenshots = resolve(process.env.RUNNER_TEMP, `about-screenshots-${frontend}`)
const errors = []
const packageName = 'anyflows-0.3.1-linux-amd64.tar.gz'
const notes = [
  '## 更新亮点',
  '',
  '- **更清晰的版本状态**',
  '- 修复更新检查反馈',
  '  - 支持嵌套说明',
  '',
  '> 更新完成后可继续使用。',
  '',
  '| 功能 | 说明 |',
  '| --- | --- |',
  '| 更新检查 | 有结果反馈 |',
  '',
  '使用 `AnyFlows` 查看当前版本。',
  '',
  '```text',
  `long-release-note-${'example-'.repeat(30)}`,
  '```',
  '',
  '[完整说明](https://example.com/notes)',
  '',
  '<script>window.__releaseNoteScriptExecuted = true</script>',
  '',
  '![tracking image](https://tracking.invalid/image.png)',
  '',
  '## SHA-256',
  '',
  '```text',
  `${'a'.repeat(64)}  ${packageName}`,
  '```',
].join('\n')
const latest = {
  version: '0.3.1', tag: 'v0.3.1', url: 'https://example.com/releases/v0.3.1',
  notes, published_at: '2026-10-25T00:00:00Z', package_name: packageName,
  package_url: `https://example.com/download/${packageName}`,
}
const initial = {
  edition: 'community', version: '0.3.0-beta.6', core_version: '0.3.0-beta.6',
  commit: 'a'.repeat(40), build_time: '2026-10-25T00:00:00Z',
  target: 'x86_64-unknown-linux-gnu', deployment: 'binary', node_id: 'ci-about-node',
  launch_mode: 'launcher', launcher_protocol: 1, launcher_version: '0.3.0-beta.6',
  update_phase: 'idle', check_enabled: true, install_enabled: true,
  restart_required: false, update_available: false,
}
let status = initial
let checkFailure = false
let checkResult
let releaseCheck
let releaseLoad
let initialLoad = new Promise(resolve => { releaseLoad = resolve })
let pendingCheck
let installs = []
let trackingRequests = 0
let server
let browser
let page
let serverOutput = ''

try {
  await mkdir(screenshots, { recursive: true })
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>About CI</title><div id="root"></div><script type="module" src="/.about-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { AboutPage } from './src/features/about/about-page';
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '20px', maxWidth: '1240px', margin: 'auto' }}><AboutPage /></main>${isNext ? '</HeroUIProvider>' : ''}
      </QueryClientProvider>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.about-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, serverOutput)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => localStorage.setItem('anyflows.locale', 'zh'))
  await page.route('https://tracking.invalid/**', async route => {
    trackingRequests += 1
    await route.abort()
  })
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const path = new URL(route.request().url()).pathname
    if (path === '/api/admin/software') {
      if (initialLoad) await initialLoad
    } else if (path === '/api/admin/software/check') {
      if (pendingCheck) await pendingCheck
      if (checkFailure) {
        await route.fulfill({ status: 503, contentType: 'application/json', body: JSON.stringify({ code: 'release_check_failed' }) })
        return
      }
      status = checkResult ?? status
    } else if (path === '/api/admin/software/install') {
      installs.push(route.request().postDataJSON())
      status = { ...status, update_phase: 'preparing' }
    } else {
      errors.push(`Unexpected API: ${path}`)
      await route.fulfill({ status: 404, body: '{}' })
      return
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(status) })
  })
  const runCheck = async () => {
    await Promise.all([
      page.waitForResponse(response => new URL(response.url()).pathname === '/api/admin/software/check'),
      page.getByRole('button', { name: '检查更新', exact: true }).click(),
    ])
    await page.getByRole('button', { name: '检查更新', exact: true }).waitFor()
  }
  await page.goto(`http://127.0.0.1:${port}/.about-smoke.html`)
  await page.getByText('正在读取版本信息…', { exact: true }).waitFor()
  releaseLoad()
  initialLoad = undefined
  await page.getByRole('button', { name: '检查更新', exact: true }).waitFor()

  pendingCheck = new Promise(resolve => { releaseCheck = resolve })
  checkResult = { ...initial, checked_at: 1_790_899_200, update_available: true, latest }
  await page.getByRole('button', { name: '检查更新', exact: true }).click()
  const checking = page.getByRole('button', { name: '正在检查…', exact: true })
  await checking.waitFor()
  assert.ok(await checking.isDisabled(), 'A pending check cannot be submitted twice')
  releaseCheck()
  pendingCheck = undefined
  await page.getByText('发现新版本 0.3.1，可查看下方更新内容。', { exact: true }).waitFor()
  const releaseNotes = page.locator('details').filter({ hasText: '更新内容' }).first()
  await releaseNotes.getByRole('heading', { name: '更新亮点', exact: true }).waitFor()
  await releaseNotes.locator('summary').click()
  assert.equal(await releaseNotes.getByRole('heading', { name: '更新亮点', exact: true }).isVisible(), false, 'Release notes can be collapsed')
  await releaseNotes.locator('summary').click()
  await releaseNotes.getByRole('heading', { name: '更新亮点', exact: true }).waitFor()
  assert.equal(await page.getByRole('table').count(), 1, 'GFM tables render as tables')
  assert.ok(await page.locator('blockquote').isVisible(), 'Quotes render as quotes')
  assert.ok(await page.locator('pre code').isVisible(), 'Fenced code renders as a code block')
  assert.ok(await page.locator('li li').count() > 0, 'Nested lists retain their structure')
  const releaseLink = page.getByRole('link', { name: '完整说明', exact: true })
  assert.equal(await releaseLink.getAttribute('target'), '_blank')
  assert.match(await releaseLink.getAttribute('rel'), /noopener/)
  assert.equal(await page.evaluate(() => Boolean(window.__releaseNoteScriptExecuted)), false)
  assert.equal(trackingRequests, 0, 'Release notes do not request remote tracking images')
  assert.equal(await page.getByText(packageName, { exact: false }).count(), 0, 'Packages and checksums are not shown in the release summary')
  assert.equal(await page.getByText('anyflows update install', { exact: false }).count(), 0, 'Installation commands are not shown')
  assert.equal(await page.locator('a[href*="/download/"]').count(), 0, 'Package download links are not shown')
  assert.ok(await page.getByRole('button', { name: '立即更新', exact: true }).isDisabled(), 'Installation requires backup confirmation')

  for (const theme of ['dark', 'light']) {
    await page.evaluate(mode => {
      document.documentElement.classList.remove('dark', 'light')
      document.documentElement.classList.add(mode)
    }, theme)
    for (const [name, width, height] of [['desktop', 1280, 960], ['tablet', 768, 1024], ['mobile', 390, 844], ['small-mobile', 320, 740]]) {
      await page.setViewportSize({ width, height })
      await page.evaluate(() => document.fonts.ready)
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${frontend} ${theme} overflows at ${width}px`)
      assert.equal(await page.getByRole('heading', { name: '云上极致（深圳）互联网有限公司', exact: true }).evaluate(element => getComputedStyle(element).textAlign), 'center')
      assert.equal(await page.getByRole('heading', { name: '共建 AnyFlows 的伙伴', exact: true }).evaluate(element => getComputedStyle(element).textAlign), 'center')
      if (name === 'desktop' || name === 'mobile') {
        await page.screenshot({ path: resolve(screenshots, `${theme}-${name}.png`), fullPage: true, animations: 'disabled' })
      }
    }
  }
  await page.setViewportSize({ width: 1280, height: 960 })

  checkFailure = true
  await runCheck()
  await page.getByRole('alert').filter({ hasText: '检查更新失败' }).waitFor()
  assert.equal(await page.getByText('发现新版本 0.3.1，可查看下方更新内容。', { exact: true }).count(), 0, 'A failed check clears the previous success feedback')
  checkFailure = false
  checkResult = { ...initial, checked_at: 1_790_899_260, latest }
  await runCheck()
  await page.getByText('检查完成，暂未发现新版本。', { exact: true }).waitFor()
  assert.equal(await page.getByRole('alert').count(), 0, 'A successful retry clears the previous error')
  assert.equal(await page.getByRole('button', { name: '立即更新', exact: true }).count(), 0)

  checkResult = { ...initial, checked_at: 1_790_899_320, update_available: true, latest }
  await runCheck()
  await page.getByRole('checkbox').check()
  await runCheck()
  await page.getByText('发现新版本 0.3.1，可查看下方更新内容。', { exact: true }).waitFor()
  assert.equal(await page.getByRole('checkbox').isChecked(), false, 'A new check resets installation consent')
  await page.getByRole('checkbox').check()
  await Promise.all([
    page.waitForResponse(response => new URL(response.url()).pathname === '/api/admin/software/install'),
    page.getByRole('button', { name: '立即更新', exact: true }).click(),
  ])
  await page.getByText(/启动器托管更新中/).waitFor()
  assert.deepEqual(installs, [{ tag: 'v0.3.1', expected_node_id: 'ci-about-node' }], 'Installation remains bound to the selected release and current node')
  status = { ...status, version: '0.3.1', update_phase: 'idle', update_available: false }
  await page.getByRole('button', { name: '刷新页面', exact: true }).waitFor()
  assert.equal(installs.length, 1, 'Restart polling does not repeat installation')
  assert.deepEqual(errors, [])
  console.log(`About browser checks passed for ${frontend}: update results, recovery, safe Markdown, confirmation, launcher polling, and responsive themes.`)
} catch (error) {
  if (page) await page.screenshot({ path: resolve(screenshots, 'failure.png'), fullPage: true, animations: 'disabled' }).catch(() => {})
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  releaseLoad?.()
  releaseCheck?.()
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
