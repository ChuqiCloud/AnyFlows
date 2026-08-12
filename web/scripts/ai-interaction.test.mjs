import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

function source(relativePath) {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), 'utf8')
}

const activitySource = source('../src/components/ai/ai-activity.tsx')
const indexCss = source('../src/index.css')
const viteConfig = source('../vite.config.ts')

test('AI 活动组件只承载视觉状态且支持三种密度', () => {
  assert.match(activitySource, /size\?: 'compact' \| 'default' \| 'panel'/)
  assert.match(activitySource, /不承载任何业务状态机/)
  assert.doesNotMatch(activitySource, /fetch\(|useMutation|useQuery|setInterval/)
})

test('AI 动效只使用合成器属性并提供减少动态效果降级', () => {
  const motionBlock = indexCss.slice(indexCss.indexOf('.ai-activity-pixel'))

  assert.match(motionBlock, /transform:/)
  assert.match(motionBlock, /opacity:/)
  assert.doesNotMatch(motionBlock, /background-position:/)
  assert.doesNotMatch(motionBlock, /transition:\s*all/)
  assert.match(motionBlock, /data-motion='reduced'/)
  assert.match(motionBlock, /data-ai-motion='move'/)
  assert.match(motionBlock, /prefers-reduced-motion:\s*reduce/)
  assert.match(motionBlock, /animation:\s*none\s*!important/)
})

test('本地前端通过同源代理连接独立后端', () => {
  assert.match(viteConfig, /VITE_API_PROXY_TARGET/)
  assert.match(viteConfig, /const apiProxyOrigin = new URL\(apiProxyTarget\)\.origin/)
  assert.match(viteConfig, /changeOrigin: true/)
  assert.match(viteConfig, /Origin: apiProxyOrigin/)
  assert.match(viteConfig, /'\/api': createApiProxy\(\)/)
  assert.match(viteConfig, /'\/v1': createApiProxy\(\)/)
  assert.match(viteConfig, /127\.0\.0\.1:8085/)
})
