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
const isNext = frontend === 'web-next'
const port = isNext ? 4174 : 4173
const html = resolve(directory, '.usage-logs-smoke.html')
const entry = resolve(directory, '.usage-logs-smoke.tsx')
const baseUrl = `http://127.0.0.1:${port}/.usage-logs-smoke.html`
const errors = []
const requests = []
let server
let browser
let serverOutput = ''

const owner = {
  id: 41, username: 'failure-owner', email: 'failure-owner@example.test',
  role: 'user', status: 'enabled', default_group_id: 51,
  quota: 80_000, used_quota: 20_000, frozen_quota: 1_000,
  request_count: 17, rpm_limit: 60, concurrency: 2,
}
// Exercises defensive layout for legacy/imported data beyond the current API limit.
const longError = `上游返回错误\n${'invalid_payload_'.repeat(120)}\n错误末尾：请检查工具消息关联。`
const failed = {
  id: 101, request_id: 'request-failed/a?b=x y', model: 'failed-model',
  protocol: 'anthropic', operation: 'chat', error_kind: 'upstream_response_internal',
  error_code: 'upstream_bad_request', error_message: longError,
  user_id: owner.id, username: owner.username, token_id: 61, group_id: 51,
  organization_id: 81, organization_team_id: 91, channel_id: 71,
  duration_ms: 1_350, created_at: 1_790_899_203,
}
const noOwner = {
  ...failed, id: 102, request_id: 'request-before-auth', model: 'anonymous-model',
  user_id: null, username: null, token_id: null, group_id: null,
  organization_id: null, organization_team_id: null, channel_id: null,
  error_code: 'authentication_failed', error_message: '请求未通过认证',
  duration_ms: 0, created_at: 1_790_899_202,
}
const deletedOwner = {
  ...failed, id: 103, request_id: 'request-deleted-owner', model: 'deleted-owner-model',
  user_id: 42, username: null, error_message: '已删除用户的失败请求',
  created_at: 1_790_899_201,
}
const settled = {
  id: 100, event_id: '00000000000000000000000000000064', request_id: 'request-settled',
  user_id: owner.id, username: owner.username, token_id: 61, group_id: 51,
  organization_id: null, organization_team_id: null, model: 'settled-model',
  protocol: 'open_ai_responses', operation: 'responses', is_stream: true,
  reasoning_effort: 'high', reasoning_budget_tokens: 1_024,
  first_token_ms: 500, duration_ms: 1_200, billing_mode: 'per_token',
  input_tokens: 400, output_tokens: 100, cache_read: 300,
  cache_creation_5m: 0, cache_creation_1h: 0, reasoning_tokens: 40,
  audio_input_tokens: 0, audio_output_tokens: 0, audio_duration_nanoseconds: null,
  video_duration_seconds: null, video_resolution: null,
  usage_source: 'upstream', usage_semantics: 'inclusive', quota: 2_100,
  created_at: 1_790_899_200,
}
const adminResponse = {
  logs: [settled], failed_logs: [failed, noOwner, deletedOwner],
  next_cursor: null, failed_next_cursor: null,
}
const userResponse = {
  logs: [{
    id: settled.id, token_id: settled.token_id, request_id: settled.request_id,
    model: settled.model, protocol: settled.protocol, operation: settled.operation,
    is_stream: settled.is_stream, reasoning_effort: settled.reasoning_effort,
    reasoning_budget_tokens: settled.reasoning_budget_tokens,
    first_token_ms: settled.first_token_ms, duration_ms: settled.duration_ms,
    billing_mode: settled.billing_mode, input_tokens: settled.input_tokens,
    output_tokens: settled.output_tokens, cache_read: settled.cache_read,
    cache_creation_5m: 0, cache_creation_1h: 0, reasoning_tokens: settled.reasoning_tokens,
    quota: settled.quota, created_at: settled.created_at,
  }],
  failed_logs: [{
    id: failed.id, request_id: failed.request_id, model: failed.model,
    protocol: failed.protocol, operation: failed.operation, duration_ms: failed.duration_ms,
    error_code: failed.error_code, error_message: longError, created_at: failed.created_at,
  }],
  next_cursor: null, failed_next_cursor: null,
}

function row(page, model) {
  return page.locator('article').filter({ has: page.getByText(model, { exact: true }) })
}

async function openFailure(page, model, method = 'click') {
  const item = row(page, model)
  await item.waitFor()
  if (method === 'click') await item.click()
  else {
    await item.getByRole('button', { name: model, exact: true }).focus()
    await page.keyboard.press(method)
  }
  const dialog = page.getByRole('dialog')
  await dialog.getByRole('heading', { name: model, exact: true }).waitFor()
  return dialog
}

async function closeDialog(page) {
  await page.keyboard.press('Escape')
  await page.waitForFunction(() => document.querySelectorAll('[role="dialog"]').length === 0)
}

async function checkLayout(page, dialog, label) {
  await page.evaluate(() => document.fonts.ready)
  if (dialog) {
    const geometry = await dialog.evaluate(async node => {
      const started = performance.now()
      const deadline = started + 5_000
      let previous
      let stableFrames = 0
      let stable = false
      let activeAnimations = []
      while (performance.now() < deadline) {
        await new Promise(resolve => requestAnimationFrame(resolve))
        const rect = node.getBoundingClientRect()
        const current = [rect.left, rect.right, rect.top, rect.bottom, node.clientWidth, node.scrollWidth]
        activeAnimations = node.getAnimations({ subtree: true }).filter(animation => (
          (animation.playState === 'running' || animation.pending)
          && Number.isFinite(animation.effect?.getComputedTiming().endTime)
        ))
        stableFrames = previous && current.every((value, index) => Math.abs(value - previous[index]) < 0.1) ? stableFrames + 1 : 0
        previous = current
        if (stableFrames >= 4 && activeAnimations.length === 0) {
          stable = true
          break
        }
      }
      const rect = node.getBoundingClientRect()
      const describe = element => {
        const bounds = element.getBoundingClientRect()
        return {
          tag: element.tagName.toLowerCase(), role: element.getAttribute('role'),
          className: String(element.className).slice(0, 180),
          left: bounds.left, right: bounds.right, width: bounds.width,
          clientWidth: element.clientWidth, scrollWidth: element.scrollWidth,
          scrollHeight: element.scrollHeight,
          overflowX: getComputedStyle(element).overflowX,
          text: element.textContent?.trim().slice(0, 80),
        }
      }
      const overflowingChildren = [...node.querySelectorAll('*')]
        .filter(element => {
          const bounds = element.getBoundingClientRect()
          return bounds.width > 0 && bounds.height > 0 && (
            bounds.left < rect.left - 1 || bounds.right > rect.right + 1
            || element.scrollWidth > element.clientWidth + 1
          )
        })
        .map(describe)
        .sort((left, right) => (right.scrollWidth - right.clientWidth) - (left.scrollWidth - left.clientWidth) || right.width - left.width)
        .slice(0, 6)
      return {
        stable, stableFrames, elapsedMs: Math.round(performance.now() - started),
        viewport: { width: innerWidth, height: innerHeight, documentScrollWidth: document.documentElement.scrollWidth },
        rect: { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom, width: rect.width, height: rect.height },
        clientWidth: node.clientWidth, scrollWidth: node.scrollWidth, scrollHeight: node.scrollHeight,
        activeAnimations: activeAnimations.map(animation => ({ playState: animation.playState, currentTime: animation.currentTime, endTime: animation.effect?.getComputedTiming().endTime })),
        overflowingChildren,
      }
    })
    const diagnostic = `${label}: ${JSON.stringify(geometry)}`
    assert.equal(geometry.stable, true, `Details did not reach a stable layout within 5 seconds. ${diagnostic}`)
    assert.ok(geometry.rect.left >= -1 && geometry.rect.right <= geometry.viewport.width + 1, `Details extend outside the viewport. ${diagnostic}`)
    assert.ok(geometry.scrollWidth <= geometry.clientWidth + 1, `Details content overflows horizontally. ${diagnostic}`)
    assert.ok(geometry.viewport.documentScrollWidth <= geometry.viewport.width, `Page overflows. ${diagnostic}`)
  } else {
    const geometry = await page.evaluate(() => ({ viewportWidth: innerWidth, viewportHeight: innerHeight, scrollWidth: document.documentElement.scrollWidth }))
    assert.ok(geometry.scrollWidth <= geometry.viewportWidth, `${label}: page overflows. ${JSON.stringify(geometry)}`)
  }
}

try {
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Usage logs CI</title><div id="root"></div><script type="module" src="/.usage-logs-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react'; import { BrowserRouter } from 'react-router-dom'; import { RouterNavigationBridge } from './src/lib/router-navigation';" : "import { TooltipProvider } from './src/components/ui/tooltip';"}
    import './src/index.css';
    import './src/i18n';
    import { UsageLogPage } from './src/features/usage-logs/usage-log-page';
    import { UsageLogTable } from './src/features/usage-logs/usage-log-table';
    import type { AdminFailedCallLog } from './src/lib/api/generated/types.gen';
    const mode = new URLSearchParams(location.search).get('mode');
    const internalRows = ${JSON.stringify(adminResponse.failed_logs)} satisfies AdminFailedCallLog[];
    const content = mode === 'hidden-principal' || mode === 'readonly'
      ? <UsageLogTable logs={internalRows} admin={mode === 'hidden-principal'} showPrincipal={mode === 'readonly'} filtered={false} formatQuota={String} onSelect={() => {}} onUserSelect={() => { throw new Error('Hidden principal must not open user details'); }} />
      : <UsageLogPage admin={mode !== 'user'} />;
    createRoot(document.getElementById('root')!).render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })}>
        ${isNext ? '<HeroUIProvider><BrowserRouter><RouterNavigationBridge />' : '<TooltipProvider>'}<main style={{ padding: '16px', maxWidth: '1240px', margin: 'auto' }}>{content}</main>${isNext ? '</BrowserRouter></HeroUIProvider>' : '</TooltipProvider>'}
      </QueryClientProvider>
    );
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8_000) })
  let ready = false
  for (let attempt = 0; attempt < 90; attempt += 1) {
    try { if ((await fetch(baseUrl)).ok) { ready = true; break } } catch {}
    await delay(1_000)
  }
  assert.ok(ready, `Vite failed to become ready: ${serverOutput}`)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  const page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write'])
  page.setDefaultTimeout(15_000)
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => localStorage.setItem('anyflows.locale', 'zh'))
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const url = new URL(request.url())
    requests.push({ method: request.method(), path: url.pathname })
    assert.equal(request.method(), 'GET', 'Log inspection must not write account or billing data')
    let response
    if (url.pathname === '/api/admin/usage-logs') response = adminResponse
    else if (url.pathname === '/api/account/usage-logs') response = userResponse
    else if (url.pathname === '/api/admin/users/41') response = owner
    else if (url.pathname === '/api/admin/users/42') {
      await route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ code: 'not_found', message: '用户不存在' }) })
      return
    } else if (url.pathname === '/api/site') {
      response = { balance_display: { mode: 'quota', unit_name: '', unit_symbol: '', quota_units_per_display_unit: '10000', symbol_position: 'suffix', fraction_digits: 0 } }
    } else {
      errors.push(`Unexpected API: ${url.pathname}`)
      await route.fulfill({ status: 404, body: '{}' })
      return
    }
    await route.fulfill({ contentType: 'application/json', body: JSON.stringify(response) })
  })

  await page.goto(baseUrl)
  await row(page, failed.model).waitFor()
  assert.equal(await page.locator('article').count(), 4)
  assert.equal(await row(page, failed.model).getByRole('button', { name: /failure-owner/ }).count(), 1, 'Failed row identifies its requester')
  assert.equal(requests.filter(request => request.path.startsWith('/api/admin/users/')).length, 0, 'User profiles are loaded only when opened')
  await row(page, failed.model).getByRole('button', { name: '复制请求 ID', exact: true }).click()
  await row(page, failed.model).getByRole('button', { name: '请求 ID 已复制', exact: true }).waitFor()
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), failed.request_id)
  assert.equal(await page.getByRole('dialog').count(), 0, 'Copying the request ID must not open details')
  await row(page, failed.model).getByRole('button', { name: /failure-owner/ }).click()
  await page.getByRole('dialog').getByRole('heading', { name: owner.username, exact: true }).waitFor()
  await page.getByRole('dialog').getByText(owner.email, { exact: true }).waitFor()
  assert.equal(await page.getByRole('heading', { name: failed.model, exact: true }).count(), 0, 'User action must not also open failure details')
  await closeDialog(page)

  let dialog = await openFailure(page, failed.model)
  assert.equal(await dialog.getByText(longError, { exact: true }).textContent(), longError, 'Details retain the complete multiline error')
  await dialog.getByText(failed.error_code, { exact: true }).waitFor()
  await dialog.getByText(failed.error_kind, { exact: true }).waitFor()
  const errorField = dialog.locator('dl > div').filter({ has: page.getByText('错误说明', { exact: true }) })
  await errorField.getByRole('button', { name: '复制内容', exact: true }).click()
  await errorField.getByRole('button', { name: '内容已复制', exact: true }).waitFor()
  assert.equal(await page.evaluate(() => navigator.clipboard.readText()), longError, 'Copying an error retains line breaks and the complete tail')
  await dialog.getByRole('button', { name: `${owner.username} (#${owner.id})`, exact: true }).click()
  await page.getByRole('dialog').getByRole('heading', { name: owner.username, exact: true }).waitFor()
  assert.equal(await page.getByRole('heading', { name: failed.model, exact: true }).count(), 0, 'Opening requester closes the failure drawer')
  await closeDialog(page)

  dialog = await openFailure(page, failed.model, 'Enter')
  await dialog.getByRole('button', { name: failed.request_id, exact: true }).click()
  await page.waitForFunction(({ next, requestId }) => next
    ? `${location.pathname}${location.search}` === `/console/debug-traces?request_id=${encodeURIComponent(requestId)}`
    : location.hash === `#/console/debug-traces?request_id=${encodeURIComponent(requestId)}`, { next: isNext, requestId: failed.request_id })
  await page.waitForFunction(() => document.querySelectorAll('[role="dialog"]').length === 0)
  dialog = await openFailure(page, failed.model, 'Space')
  await dialog.getByText(longError, { exact: true }).waitFor()
  await closeDialog(page)

  const userRequestsBeforeAnonymous = requests.filter(request => request.path.startsWith('/api/admin/users/')).length
  await row(page, noOwner.model).getByText('未记录用户', { exact: true }).waitFor()
  assert.equal(await row(page, noOwner.model).getByRole('button').filter({ hasText: '未记录用户' }).count(), 0)
  dialog = await openFailure(page, noOwner.model)
  await dialog.getByText('未记录用户', { exact: true }).waitFor()
  assert.equal(await dialog.getByRole('button', { name: /failure-owner|用户 #|#0/ }).count(), 0, 'Unknown requester must not become the current user')
  await closeDialog(page)
  assert.equal(requests.filter(request => request.path.startsWith('/api/admin/users/')).length, userRequestsBeforeAnonymous)

  await row(page, deletedOwner.model).getByRole('button').filter({ hasText: '#42' }).click()
  await page.getByRole('dialog').getByRole('heading', { name: '无法读取用户资料', exact: true }).waitFor()
  await closeDialog(page)
  dialog = await openFailure(page, deletedOwner.model)
  await dialog.getByText(deletedOwner.error_message, { exact: true }).waitFor()
  await closeDialog(page)

  await row(page, settled.model).getByRole('button', { name: `查看 ${settled.model} 调用详情`, exact: true }).filter({ visible: true }).click()
  dialog = page.getByRole('dialog')
  await dialog.getByRole('heading', { name: isNext ? '计费' : '计费事实', exact: true }).waitFor()
  await dialog.getByText('2,100', { exact: true }).waitFor()
  await dialog.getByText('400', { exact: true }).waitFor()
  await dialog.getByText('100', { exact: true }).waitFor()
  assert.equal(await dialog.getByText(longError, { exact: true }).count(), 0)
  await closeDialog(page)

  for (const theme of ['dark', 'light']) {
    await page.evaluate(mode => { document.documentElement.classList.remove('dark', 'light'); document.documentElement.classList.add(mode) }, theme)
    for (const width of [1280, 390, 320]) {
      await page.setViewportSize({ width, height: 960 })
      await checkLayout(page, undefined, `${frontend} ${theme} list at ${width}px`)
      dialog = await openFailure(page, failed.model)
      await checkLayout(page, dialog, `${frontend} ${theme} failure at ${width}px`)
      assert.equal(await dialog.getByText(longError, { exact: true }).textContent(), longError)
      await closeDialog(page)
    }
  }

  await page.setViewportSize({ width: 1280, height: 960 })
  const adminRequestsBeforeUser = requests.filter(request => request.path.startsWith('/api/admin/')).length
  await page.goto(`${baseUrl}?mode=user`)
  await row(page, failed.model).waitFor()
  assert.equal(await page.getByRole('link', { name: '调试追踪', exact: true }).count(), 0)
  dialog = await openFailure(page, failed.model)
  await dialog.getByText(longError, { exact: true }).waitFor()
  assert.equal(await dialog.getByRole('button', { name: failed.request_id, exact: true }).count(), 0, 'User request ID must not open administrator traces')
  for (const label of ['内部分类', '请求主体', '渠道', '企业', '团队']) assert.equal(await dialog.getByText(label, { exact: true }).count(), 0, `User details hide ${label}`)
  assert.equal(await dialog.getByText(owner.username, { exact: false }).count(), 0)
  await closeDialog(page)
  assert.equal(requests.filter(request => request.path.startsWith('/api/admin/')).length, adminRequestsBeforeUser, 'User logs must not request administrator APIs')

  for (const mode of ['hidden-principal', 'readonly']) {
    await page.goto(`${baseUrl}?mode=${mode}`)
    await row(page, failed.model).waitFor()
    assert.equal(await row(page, failed.model).getByText(owner.username, { exact: false }).count(), 0)
    assert.equal(await row(page, failed.model).getByText(failed.error_kind, { exact: true }).count(), 0)
    dialog = await openFailure(page, failed.model)
    await dialog.getByText(longError, { exact: true }).waitFor()
    for (const label of ['内部分类', '请求主体', '渠道', '企业', '团队']) assert.equal(await dialog.getByText(label, { exact: true }).count(), 0, `${mode} details hide ${label}`)
    assert.equal(await dialog.getByRole('button', { name: failed.request_id, exact: true }).count(), mode === 'hidden-principal' ? 1 : 0)
    assert.equal(await dialog.getByText(failed.error_kind, { exact: true }).count(), 0)
    await closeDialog(page)
  }

  assert.deepEqual(errors, [], 'Usage logs must not raise browser runtime errors or unexpected API calls')
  console.log(`Usage log browser checks passed for ${frontend}: failed-row pointer/keyboard details, requester profiles, missing/deleted users, full errors, encoded trace links, user boundaries, settled billing and responsive themes.`)
} catch (error) {
  console.error(serverOutput)
  console.error(JSON.stringify({ frontend, browserErrors: errors, requests }))
  throw error
} finally {
  if (browser) await browser.close()
  if (server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html), unlink(entry)])
}
