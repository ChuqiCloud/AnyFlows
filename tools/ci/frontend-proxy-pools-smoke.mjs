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
    const cache = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
    Object.assign(window, { profileCache: cache });
    function App() { return <main style={{padding:16}}><ProxyPoolPanel /></main>; }
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
  await page.route(url => url.pathname.startsWith('/api/'), async route => {
    const request = route.request()
    const url = new URL(request.url())
    const respond = data => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(data) })
    if (url.pathname === '/api/admin/channels') return respond({channels:[{id:7,name:'CI Channel'}],next_cursor:null})
    if (url.pathname === '/api/admin/proxy-pools' && request.method() === 'GET') return respond({pools})
    if (url.pathname === '/api/admin/proxy-pools' && request.method() === 'POST') {
      saved = request.postDataJSON()
      pools = [{id:10,name:saved.name,subscribed:false,source_host:null,interval_secs:900,default_scheme:'http',trust_proxy_dns:false,probe_url:saved.probe_url,revision:1,last_refresh_at:1,last_check_at:null,status:'pending',busy:false,channel_ids:[],nodes:[{id:11,scheme:'http',host:'192.0.2.1',port:8080,password_configured:true,healthy:false,latency_ms:null,checked_at:null,remark:'hk',enabled:true}]}]
      return route.fulfill({status:201,contentType:'application/json',body:JSON.stringify({pool:pools[0],skipped:1,duplicates:1})})
    }
    if (url.pathname === '/api/admin/proxy-pool-bindings') {binding=request.postDataJSON();pools[0].channel_ids=[binding.channel_id];return route.fulfill({status:204})}
    if (url.pathname.endsWith('/check')) {pools[0].nodes[0].healthy=true;pools[0].nodes[0].latency_ms=23;pools[0].nodes[0].checked_at=1;pools[0].status='ready';return respond(pools[0])}
    throw new Error(`Unexpected proxy pool request: ${request.method()} ${url.pathname}`)
  })
  await page.goto(`http://127.0.0.1:${port}/.proxy-pools-smoke.html`)
  await page.getByRole('button',{name:'新建代理池',exact:true}).click()
  await page.getByLabel('池名称',{exact:true}).fill('CI Proxy Pool')
  const content='http://ci-user:ci-password@192.0.2.1:8080 #hk\ninvalid\nhttp://ci-user:ci-password@192.0.2.1:8080'
  await page.locator('input[type=file]').setInputFiles({name:'pool.txt',mimeType:'text/plain',buffer:Buffer.from(content)})
  await page.getByRole('button',{name:'保存',exact:true}).click()
  await page.getByRole('heading',{name:/CI Proxy Pool/}).waitFor()
  assert.equal(saved.content,content)
  assert.equal(saved.source_url,null)
  await page.getByText(/跳过 1 条无效项，合并 1 条重复项/).waitFor()
  assert.equal((await page.locator('body').innerText()).includes('ci-password'),false)
  const choose = async (label,value,text) => {
    if (isNext) {await page.getByRole('button',{name:new RegExp(label)}).click();await page.getByRole('option',{name:text,exact:true}).click();await page.getByRole('listbox').waitFor({state:'hidden'})}
    else await page.getByRole('combobox',{name:label,exact:true}).selectOption(value)
  }
  await Promise.all([
    page.waitForResponse(response => response.url().endsWith('/api/admin/proxy-pool-bindings') && response.status() === 204),
    choose('使用代理池','10','CI Proxy Pool'),
  ])
  await page.getByRole('button',{name:'测活',exact:true}).click()
  await page.getByText('健康 · 23 ms',{exact:true}).waitFor()
  assert.deepEqual(binding,{channel_id:0,pool_id:10})
  await choose('节点状态','failed','未通过')
  await page.getByText('暂无匹配节点',{exact:true}).waitFor()
  await page.setViewportSize({width:390,height:844})
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth),false,'Pool controls must fit a mobile viewport')
  assert.deepEqual(errors,[])
  const screenshot=await page.screenshot({fullPage:true,animations:'disabled'})
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_proxy_pools_BEGIN`)
  console.log(screenshot.toString('base64').match(/.{1,120}/g).join('\n'))
  console.log(`ANYFLOWS_SCREENSHOT_${frontend}_proxy_pools_END`)
  console.log(`Proxy pool browser checks passed for ${frontend}: file import, import feedback, global binding, health checks, node filters and mobile layout.`)
} catch (error) {
  if(page) console.error((await page.locator('body').ariaSnapshot()).slice(-8000))
  throw error
} finally {
  if(browser) await browser.close()
  if(server) server.kill('SIGTERM')
  await Promise.allSettled([unlink(html),unlink(entry)])
}
