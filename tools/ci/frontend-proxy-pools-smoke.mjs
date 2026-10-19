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
const html = resolve(directory, '.proxy-pools-smoke.html')
const entry = resolve(directory, '.proxy-pools-smoke.tsx')
const port = frontend === 'web' ? 4173 : 4174
const isNext = frontend === 'web-next'
let server
let browser
let page
let serverOutput = ''
const errors = []

try {
  await writeFile(html, '<!doctype html><html lang="zh" class="dark"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Proxy pools CI</title><div id="root"></div><script type="module" src="/.proxy-pools-smoke.tsx"></script></html>')
  await writeFile(entry, `
    import React from 'react';
    import { createRoot } from 'react-dom/client';
    import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
    ${isNext ? "import { HeroUIProvider } from '@heroui/react';" : ''}
    import './src/index.css';
    import './src/i18n';
    import { ProxyPoolPanel } from './src/features/credential-proxies/proxy-pool-panel';
    import { ProxyCleanupControls } from './src/features/credential-proxies/proxy-cleanup-controls';
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
    Object.assign(window, { profileCache: cache });
    function App() { return <main style={{padding:16}}><ProxyPoolPanel /><ProxyCleanupControls target="pools" selected={[99]} onDeleted={() => {}} /><ProxyCleanupControls target="nodes" selected={[]} unused onDeleted={() => {}} /></main>; }
    createRoot(document.getElementById('root')!).render(<QueryClientProvider client={cache}>${isNext ? '<HeroUIProvider>' : ''}<App />${isNext ? '</HeroUIProvider>' : ''}</QueryClientProvider>);
  `)
  server = spawn(process.execPath, [resolve(directory, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', String(port), '--strictPort'], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] })
  server.stdout.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  server.stderr.on('data', chunk => { serverOutput = (serverOutput + chunk).slice(-8000) })
  let ready = false
  for (let attempt = 0; attempt < 60; attempt += 1) {
    try { if ((await fetch(`http://127.0.0.1:${port}/.proxy-pools-smoke.html`)).ok) { ready = true; break } } catch {}
    await delay(1000)
  }
  assert.ok(ready, `Vite failed to become ready: ${serverOutput}`)
  browser = await chromium.launch({ headless: true, args: ['--no-sandbox'] })
  page = await browser.newPage({ viewport: { width: 1280, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' })
  page.setDefaultTimeout(12_000)
  page.setDefaultNavigationTimeout(45_000)
  page.on('pageerror', error => errors.push(error.message))
  await page.addInitScript(() => { localStorage.setItem('anyflows.locale', 'zh') })


  let pools = []
  let saved
  let binding
  const cleanupPreviews = []
  const cleanupExecutions = []
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const url = new URL(request.url())
    const respond = data => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) })
    if (url.pathname === '/api/admin/channels') return respond({channels:[{id:7,name:'CI Channel'}],next_cursor:null})
    if (url.pathname === '/api/admin/proxy-pools' && request.method() === 'GET') return respond({pools})
    if (((url.pathname === '/api/admin/proxy-pools' && request.method() === 'POST') || (url.pathname === '/api/admin/proxy-pools/10' && request.method() === 'PUT'))) {
      saved = request.postDataJSON()
      pools = [{id:10,name:saved.name,subscribed:saved.subscribed,source_host:saved.subscribed?'subscription.example':null,interval_secs:saved.interval_secs,default_scheme:'http',trust_proxy_dns:false,probe_url:saved.probe_url,revision:request.method()==='PUT'?2:1,last_refresh_at:1,last_refresh_attempt_at:null,last_check_at:null,status:'pending',busy:false,channel_ids:[],nodes:[{id:11,scheme:'http',host:'192.0.2.1',port:8080,password_configured:true,healthy:false,latency_ms:null,checked_at:null,remark:'hk',enabled:true},{id:12,scheme:'http',host:'192.0.2.2',port:8080,password_configured:false,healthy:false,latency_ms:null,checked_at:1,remark:'failed-1',enabled:true},{id:13,scheme:'http',host:'192.0.2.3',port:8080,password_configured:false,healthy:false,latency_ms:null,checked_at:1,remark:'failed-2',enabled:true}]}]
      return route.fulfill({status:request.method()==='POST'?201:200,contentType:'application/json',body:JSON.stringify({pool:pools[0],skipped:request.method()==='POST'?1:0,duplicates:request.method()==='POST'?1:0})})
    }
    if (url.pathname === '/api/admin/proxy-pool-bindings') {binding=request.postDataJSON();pools[0].channel_ids=[binding.channel_id];return route.fulfill({status:204})}
    if (url.pathname === '/api/admin/proxy-pool-cleanup') {
      const body=request.postDataJSON()
      if (body.preview) {
        cleanupPreviews.push(body)
        const protectedIds=body.mode==='selected'&&body.ids.includes(99)?[99]:[]
        const eligible=protectedIds.length?[]:body.mode==='failed'?(body.after_id===null?[12]:[13]):body.mode==='unused'?[14]:body.ids
        const more=body.mode==='failed'&&body.after_id===null
        return respond({eligible_ids:eligible,deleted_ids:[],protected_ids:protectedIds,skipped_ids:[],more_available:more,next_after_id:more?12:null})
      }
      cleanupExecutions.push(body)
      pools[0].nodes=pools[0].nodes.filter(node=>!body.ids.includes(node.id))
      return respond({eligible_ids:[],deleted_ids:body.ids,protected_ids:[],skipped_ids:[],more_available:false,next_after_id:null})
    }
    if (url.pathname.endsWith('/check')) {pools[0].nodes[0].healthy=true;pools[0].nodes[0].latency_ms=23;pools[0].nodes[0].checked_at=1;pools[0].status='ready';return respond(pools[0])}
    throw new Error(`Unexpected proxy pool request: ${request.method()} ${url.pathname}`)
  })
  const choose = async (label,value,text) => {
    if (isNext) {await page.getByRole('button',{name:new RegExp(label)}).click();await page.getByRole('option',{name:text,exact:true}).click();await page.getByRole('listbox').waitFor({state:'hidden'})}
    else await page.getByRole('combobox',{name:label,exact:true}).selectOption(value)
  }
  await page.goto(`http://127.0.0.1:${port}/.proxy-pools-smoke.html`)
  await page.getByRole('button',{name:'新建代理池',exact:true}).click()
  await page.getByLabel('池名称',{exact:true}).fill('CI Proxy Pool')
  await page.getByRole('switch',{name:'启用 URL 订阅'}).click()
  await page.getByPlaceholder('https://example.com/proxies.txt').fill('https://subscription.example/proxies.txt')
  await choose('间隔单位','3600','小时')
  await page.getByLabel('订阅同步 / 测活间隔').fill('0.5')
  const content='http://ci-user:ci-password@192.0.2.1:8080 #hk\ninvalid\nhttp://ci-user:ci-password@192.0.2.1:8080'
  await page.locator('input[type=file]').setInputFiles({name:'pool.txt',mimeType:'text/plain',buffer:Buffer.from(content)})
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pools') && response.request().method() === 'POST' && response.status() === 201),
    page.getByRole('button',{name:'保存',exact:true}).click(),
  ])
  await page.getByRole('heading',{name:/CI Proxy Pool/}).waitFor()
  assert.equal(saved.content,content)
  assert.equal(saved.source_url,'https://subscription.example/proxies.txt')
  assert.equal(saved.interval_secs,1800)
  await page.getByText('每 30 分钟同步 / 测活',{exact:true}).waitFor()
  await page.getByText(/跳过 1 条无效项，合并 1 条重复项/).waitFor()
  assert.equal((await page.locator('body').innerText()).includes('ci-password'),false)
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-bindings') && response.status() === 204),
    choose('使用代理池','10','CI Proxy Pool'),
  ])
  await page.getByRole('button',{name:'测活',exact:true}).click()
  await page.getByText('健康 · 23 ms',{exact:true}).waitFor()
  assert.deepEqual(binding,{channel_id:0,pool_id:10})
  await page.getByRole('button',{name:'编辑',exact:true}).click()
  assert.equal(await page.getByLabel('订阅同步 / 测活间隔').inputValue(),'30')
  await page.getByLabel('订阅同步 / 测活间隔').fill('60')
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pools/10') && response.request().method() === 'PUT' && response.status() === 200),
    page.getByRole('button',{name:'保存',exact:true}).click(),
  ])
  assert.equal(saved.interval_secs,3600)
  await page.getByText('每 1 小时同步 / 测活',{exact:true}).waitFor()
  await choose('节点状态','failed','未通过')
  await page.getByText('192.0.2.2:8080',{exact:false}).waitFor()
  await page.getByRole('button',{name:'清理未通过节点',exact:true}).click()
  await page.getByText('清理预览：可删除 1 项，受保护 0 项，跳过 0 项。',{exact:true}).waitFor()
  await page.getByText('可删除 ID: #12',{exact:true}).waitFor()
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-cleanup') && response.request().postDataJSON().preview === false),
    page.getByRole('button',{name:'确认删除',exact:true}).click(),
  ])
  await page.getByText('已删除 1 项，保护 0 项，跳过 0 项。',{exact:true}).waitFor()
  assert.equal(cleanupExecutions[0].mode,'failed')
  assert.deepEqual(cleanupExecutions[0].ids,[12])
  await page.getByRole('button',{name:'预览下一批',exact:true}).click()
  await page.getByText('可删除 ID: #13',{exact:true}).waitFor()
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-cleanup') && response.request().postDataJSON().preview === false),
    page.getByRole('button',{name:'确认删除',exact:true}).click(),
  ])
  assert.equal(cleanupPreviews[1].after_id,12)
  assert.deepEqual(cleanupExecutions[1].ids,[13])
  await choose('节点状态','all','全部节点')
  await page.getByRole('checkbox',{name:'选择节点 #11'}).check()
  await page.getByRole('region',{name:'代理池',exact:true}).getByRole('button',{name:'批量删除节点',exact:true}).click()
  await page.getByText('可删除 ID: #11',{exact:true}).waitFor()
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-cleanup') && response.request().postDataJSON().preview === false),
    page.getByRole('button',{name:'确认删除',exact:true}).click(),
  ])
  assert.deepEqual(cleanupExecutions[2].ids,[11])
  await page.getByRole('button',{name:'批量删除代理池',exact:true}).last().click()
  await page.getByText('清理预览：可删除 0 项，受保护 1 项，跳过 0 项。',{exact:true}).waitFor()
  assert.equal(await page.getByRole('button',{name:'确认删除',exact:true}).isDisabled(),true)
  await page.getByRole('button',{name:'取消',exact:true}).click()
  await page.getByRole('button',{name:'清理未使用节点',exact:true}).click()
  await page.getByText('可删除 ID: #14',{exact:true}).waitFor()
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-cleanup') && response.request().postDataJSON().preview === false),
    page.getByRole('button',{name:'确认删除',exact:true}).click(),
  ])
  assert.equal(cleanupExecutions[3].mode,'unused')
  await page.setViewportSize({width:390,height:844})
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),false,'Pool controls must fit a mobile viewport')
  assert.deepEqual(errors,[])
  const screenshot=await page.screenshot({fullPage:true,animations:'disabled'})
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_proxy_pools_BEGIN`)
  console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_proxy_pools_END`)
  console.log(`Proxy pool browser checks passed for ${frontend}: file import, subscription interval create/edit, global binding, health checks, failed-node cleanup, cleanup pagination, selected deletion, reference protection, unused-node cleanup and mobile layout.`)
} catch (error) {
  if(page) console.error((await page.locator('body').ariaSnapshot()).slice(-8000))
  throw error
} finally {
  if(browser) await browser.close()
  if(server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html),unlink(entry)])
}
