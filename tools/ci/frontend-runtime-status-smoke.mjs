import assert from 'node:assert/strict'

// Reuses the dashboard browser and Vite server; no additional build or dependencies.
export async function checkRuntimeStatus(page, url, frontend) {
  const now = 1_790_899_200
  const nodes = Array.from({ length: 8 }, (_, index) => ({
    node_id: index === 1 ? `worker-${'a'.repeat(100)}` : `worker-${index + 1}`,
    instance_id: (index + 1).toString(16).padStart(32, '0'),
    version: '0.3.0', os: 'linux', started_at: now - 3_660, sampled_at: now,
    uptime_seconds: 3_660, status: index === 2 ? 'stale' : 'online',
    cpu_cores: index === 0 ? 0 : index === 1 ? null : 0.35, cpu_capacity_cores: 2,
    memory_bytes: 128 * 1024 ** 2, host_memory_bytes: 8 * 1024 ** 3,
    host_memory_used_bytes: 2 * 1024 ** 3, container_memory_limit_bytes: 1024 ** 3,
    container_memory_bytes: 256 * 1024 ** 2,
    database: { kind: index === 7 ? 'SQLite' : 'PostgreSQL', status: 'healthy', latency_ms: 0 },
    redis: { kind: 'Redis', status: 'healthy', latency_ms: 1 },
    history: Array.from({ length: 60 }, (_, sample) => ({
      sampled_at: now - (59 - sample) * 10,
      cpu_cores: sample === 20 ? null : sample / 100,
      memory_bytes: (100 + sample) * 1024 ** 2,
    })),
  }))
  const snapshot = {
    sampled_at: now, local_instance_id: nodes[0].instance_id, cluster_status: 'healthy',
    cluster_observed_at: now, nodes,
    prompt_cache: {
      sampled_at: now, period_start: now - 86_400, period_end: now,
      status: 'healthy', scope: 'public_usage_ledger', upstream_requests: 120,
      estimated_requests: 7, positive_cache_requests: 90, input_tokens: 123_456,
      cache_read_tokens: 12_345, cache_write_tokens: 1_234,
    },
  }
  let response = snapshot
  let failure = 0
  let requests = 0
  let release
  let pending = new Promise(resolve => { release = resolve })
  const handler = async route => {
    requests += 1
    if (pending) await pending
    await route.fulfill({
      status: failure || 200, contentType: 'application/json',
      body: JSON.stringify(failure ? { code: 'runtime_unavailable' } : response),
    })
  }
  await page.route('**/api/admin/runtime-status', handler)
  try {
    await page.goto(url)
    await page.getByRole('status', { name: '正在加载运行状态' }).waitFor()
    release()
    pending = undefined
    await page.locator('.runtime-node').first().waitFor()
    assert.equal(await page.locator('.runtime-node').count(), 6)
    assert.equal(await page.locator('.runtime-node-values strong').first().innerText(), '0 核')
    assert.equal(await page.locator('.runtime-cache-metrics strong').first().innerText(), '123,456')
    const count = requests
    await page.getByRole('button', { name: '下一页', exact: true }).click()
    assert.equal(await page.locator('.runtime-node').count(), 2)
    await page.locator('.runtime-node').last().click()
    await page.getByRole('heading', { name: '节点详情 · worker-8', exact: true }).waitFor()
    assert.equal(await page.locator('.runtime-dependency strong').first().innerText(), 'SQLite')
    assert.equal(requests, count, 'Node selection and pagination use the loaded snapshot')
    await page.getByRole('button', { name: '上一页', exact: true }).click()
    await page.locator('.runtime-node').nth(1).click()
    assert.equal(await page.locator('.runtime-summary .runtime-metric > strong').nth(1).innerText(), '—')

    for (const theme of ['dark', 'light']) {
      await page.evaluate(mode => {
        document.documentElement.classList.remove('dark', 'light')
        document.documentElement.classList.add(mode)
      }, theme)
      for (const width of [1280, 768, 390, 320]) {
        await page.setViewportSize({ width, height: 960 })
        await page.evaluate(() => document.fonts.ready)
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `${frontend} runtime ${theme} overflows at ${width}px`)
        assert.ok(await page.locator('.runtime-node-heading').evaluateAll(elements => elements.every(element => {
          const [title, badge] = [...element.children].map(child => child.getBoundingClientRect())
          return title.right <= badge.left + 1
        })), `Runtime node title overlaps its badge at ${width}px`)
      }
    }

    await page.setViewportSize({ width: 1280, height: 960 })
    await page.getByRole('button', { name: '下一页', exact: true }).click()
    response = { ...snapshot, nodes: [{ ...nodes[0], history: [] }], prompt_cache: { ...snapshot.prompt_cache, sampled_at: null, status: 'starting' } }
    await page.getByRole('button', { name: '刷新', exact: true }).click()
    await page.waitForFunction(() => document.querySelectorAll('.runtime-node').length === 1)
    assert.equal(await page.locator('.runtime-pagination').count(), 0, 'Shrinking node lists clamp pagination')
    assert.equal(await page.locator('.runtime-cache-metrics strong').first().innerText(), '—', 'Missing usage is not displayed as zero')
    assert.equal(await page.getByText('等待资源采样', { exact: true }).count(), 2)

    failure = 503
    await page.getByRole('button', { name: '刷新', exact: true }).click()
    await page.getByRole('alert').waitFor()
    assert.equal(await page.locator('.runtime-node').count(), 1, 'Transient errors retain the last snapshot')
    failure = 0
    response = snapshot
    await page.getByRole('button', { name: '重试', exact: true }).click()
    await page.locator('.runtime-node').nth(5).waitFor()
    await page.getByRole('alert').waitFor({ state: 'detached' })
    for (const status of [403, 401]) {
      failure = status
      await page.getByRole('button', { name: '刷新', exact: true }).click()
      await page.getByRole('alert').waitFor()
      assert.equal(await page.locator('.runtime-node').count(), 0, `HTTP ${status} hides the previous snapshot`)
    }
    console.log(`Runtime browser checks passed for ${frontend}: loading, pagination, resources, usage, recovery, access denial and responsive themes.`)
  } finally {
    release()
    await page.unroute('**/api/admin/runtime-status', handler)
  }
}
