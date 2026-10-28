import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const packageRequire = createRequire(import.meta.url)
const shadcnRequire = createRequire(packageRequire.resolve('shadcn'))
const deepmergeEntrypoints = [
  ['CommonJS', shadcnRequire.resolve('deepmerge')],
  ['UMD', shadcnRequire.resolve('deepmerge/dist/umd.js')],
].map(([name, path]) => {
  assert.match(readFileSync(path, 'utf8'), /function defineOwnProperty\(/)
  return [name, shadcnRequire(path)]
})

for (const [entrypoint, deepmerge] of deepmergeEntrypoints) {
  test(`${entrypoint}: deepmerge preserves an own __proto__ target key safely`, () => {
    const target = JSON.parse('{"__proto__":{"targetFlag":true}}')
    const result = deepmerge(target, { safe: true })

    assert.equal(Object.getPrototypeOf(result), Object.prototype)
    assert.equal(Object.hasOwn(result, '__proto__'), false)
    assert.equal(({}).targetFlag, undefined)
  })

  test(`${entrypoint}: deepmerge ignores an own __proto__ source key safely`, () => {
    const source = JSON.parse('{"__proto__":{"sourceFlag":true}}')
    const result = deepmerge({}, source)

    assert.equal(Object.getPrototypeOf(result), Object.prototype)
    assert.equal(Object.hasOwn(result, '__proto__'), false)
    assert.equal(({}).sourceFlag, undefined)
  })

  test(`${entrypoint}: deepmerge keeps own constructor data safely`, () => {
    const target = JSON.parse('{"constructor":{"prototype":{"targetFlag":true}}}')
    const result = deepmerge(target, {})

    assert.equal(Object.getPrototypeOf(result), Object.prototype)
    assert.equal(Object.hasOwn(result, 'constructor'), true)
    assert.deepEqual(result.constructor.prototype, { targetFlag: true })
    assert.equal(({}).targetFlag, undefined)
  })
}
