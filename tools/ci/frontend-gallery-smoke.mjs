import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { readFile, writeFile, unlink } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
const frontend = process.argv[2]
assert.ok(['web', 'web-next'].includes(frontend))
assert.ok(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'Browser tests run in the remote CI tool environment')
const require = createRequire(resolve(process.env.PLAYWRIGHT_PACKAGE_ROOT, 'package.json'))
const { chromium } = require('playwright')
const directory = resolve(root, frontend)
const port = frontend === 'web' ? 4173 : 4174
const html = resolve(directory, '.gallery-smoke.html')
const entry = resolve(directory, '.gallery-smoke.tsx')
const isNext = frontend === 'web-next'
const errors = []
let server
let browser
let serverOutput = ''

try {
  await writeFile(html, '<!doctype html><html lang="zh"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Frontend gallery CI</title><div id="root"></div><script type="module" src="/.gallery-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { FrontendTemplatePanel } from './src/features/site-settings/frontend-template-panel';
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        ${isNext ? '<HeroUIProvider>' : ''}<main style={{ padding: '16px', maxWidth: '1280px', margin: 'auto' }}><FrontendTemplatePanel /></main>${isNext ? '</HeroUIProvider>' : ''}
      </QueryClientProvider>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  server.stderr.on('data', (chunk) => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.gallery-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1_000)
  }
  assert.ok(ready, `Vite failed to become ready: ${serverOutput}`)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.on('pageerror', (error) => errors.push(error.message))
  await page.addInitScript(() => {
    localStorage.setItem('anyflows.locale', 'zh')
    // The real Next entrypoint applies the persisted site theme before React
    // mounts. The gallery harness has no index.html bootstrap script, so set
    // the same default explicitly; otherwise HeroUI falls back to its light
    // palette while the site tokens remain dark and the screenshot is invalid.
    const stored = localStorage.getItem('anyflows.theme')
    const mode = stored === 'light' || stored === 'dark' ? stored : 'dark'
    document.documentElement.classList.add(mode)
  })
  let activeId = null
  let catalogRequests = 0
  let previewRequests = 0
  let activations = 0
  const templates = [
    { id: 'embedded', name: 'AnyFlows Classic', builtin: true },
    { id: 'embedded-next', name: 'AnyFlows Next', builtin: true },
    ...Array.from({ length: 24 }, (_, index) => ({ id: `theme-${index}`, name: `Theme ${index}`, builtin: false })),
  ].map((template) => ({ ...template, version: '1.0', api_contract: '0.2', valid: true, error: null, author: 'CI fixture', description: null, preview_url: `/api/admin/frontend-templates/${template.id}/preview` }))
  const image = await readFile(resolve(root, 'crates/af-server/src/frontend_previews/classic.svg'))
  const nextImage = await readFile(resolve(root, 'crates/af-server/src/frontend_previews/next.svg'))
  await page.route((url) => url.pathname.startsWith('/api/'), async (route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    if (path.endsWith('/preview')) {
      previewRequests += 1
      await route.fulfill({ contentType: 'image/svg+xml', body: path.includes('/embedded-next/') ? nextImage : image })
      return
    }
    if (path.endsWith('/active')) { activations += 1; activeId = request.postDataJSON().template_id }
    else if (path === '/api/admin/frontend-templates') catalogRequests += 1
    else if (!path.endsWith('/scan')) throw new Error(`Unexpected API request: ${path}`)
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify({ active_id: activeId, templates }) })
  })
  await page.goto(`http://127.0.0.1:${port}/.gallery-smoke.html`)
  await page.getByRole('heading', { name: 'AnyFlows Classic', exact: true }).waitFor()
  await page.locator('article img').first().waitFor()
  assert.equal(await page.locator('article').count(), 8)
  assert.ok(previewRequests > 0 && previewRequests <= 8, `Initial page fetched ${previewRequests} previews`)
  assert.equal(catalogRequests, 1)
  await page.getByRole('button', { name: '下一页', exact: true }).click()
  await page.getByRole('heading', { name: 'Theme 6', exact: true }).waitFor()
  assert.equal(await page.locator('article').count(), 8)
  assert.equal(catalogRequests, 1, 'Pagination must reuse catalog metadata')
  await page.getByRole('button', { name: /内置前端/ }).click()
  await page.getByRole('heading', { name: 'AnyFlows Classic', exact: true }).waitFor()
  assert.equal(await page.locator('article').count(), 2)
  await page.getByRole('button', { name: '预览', exact: true }).first().click()
  await page.getByRole('dialog').waitFor()
  assert.equal(activations, 0, 'Preview must not activate a template')
  await page.keyboard.press('Escape')
  await page.getByRole('dialog').waitFor({ state: 'hidden' })
  await page.getByRole('button', { name: '启用', exact: true }).click()
  await page.getByRole('button', { name: '取消', exact: true }).click()
  await page.getByRole('dialog').waitFor({ state: 'hidden' })
  assert.equal(activations, 0, 'Cancel must not activate a template')

  for (const [name, width, height] of [['desktop', 1280, 960], ['mobile', 390, 844]]) {
    await page.setViewportSize({ width, height })
    await page.evaluate(() => document.fonts.ready)
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), `${frontend} overflowed at ${width}px`)
    const screenshot = await page.screenshot({ fullPage: true })
    console.log(`ANYFLOWS_SCREENSHOT_${frontend}_${name}_BEGIN`)
    console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
    console.log(`ANYFLOWS_SCREENSHOT_${frontend}_${name}_END`)
  }
  await page.getByRole('button', { name: '启用', exact: true }).click()
  await page.getByRole('button', { name: '确认启用并刷新', exact: true }).click()
  await page.getByText('当前启用：AnyFlows Next', { exact: true }).waitFor()
  assert.equal(activations, 1)
  assert.equal(activeId, 'embedded-next')
  assert.deepEqual(errors, [], 'The gallery must not raise browser runtime errors')
  console.log(`Gallery browser checks passed for ${frontend}: pagination, bounded previews, cancellation, activation, reload, desktop and mobile layout.`)
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ browserErrors: errors }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
