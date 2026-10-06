import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { runInNewContext } from 'node:vm'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const indexPath = fileURLToPath(new URL('../index.html', import.meta.url))
const indexHtml = readFileSync(indexPath, 'utf8')
const bootstrapScript = indexHtml.match(/<script>([\s\S]*?)<\/script>/)?.[1]

assert.ok(bootstrapScript, 'index.html 必须包含首帧初始化脚本')

function runBootstrap({ storedMotion, reduceMotion }) {
  const attributes = new Map()

  runInNewContext(bootstrapScript, {
    document: {
      documentElement: {
        classList: { add() {} },
        setAttribute(name, value) {
          attributes.set(name, value)
        },
      },
    },
    localStorage: {
      getItem(key) {
        return key === 'anyflows.motion' ? storedMotion : null
      },
    },
    window: {
      matchMedia(query) {
        return {
          matches: query === '(prefers-reduced-motion: reduce)' && reduceMotion,
        }
      },
    },
  })

  return attributes.get('data-motion')
}

test('未保存偏好时普通系统默认启用完整动效', () => {
  assert.equal(runBootstrap({ storedMotion: null, reduceMotion: false }), 'full')
})

test('未保存偏好时尊重系统减少动态效果设置', () => {
  assert.equal(runBootstrap({ storedMotion: null, reduceMotion: true }), 'reduced')
})

test('用户保存的动效选择优先于系统设置', () => {
  assert.equal(runBootstrap({ storedMotion: 'full', reduceMotion: true }), 'full')
  assert.equal(runBootstrap({ storedMotion: 'reduced', reduceMotion: false }), 'reduced')
})
