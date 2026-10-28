import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import test from 'node:test'

const packageRequire = createRequire(import.meta.url)
const shadcnRequire = createRequire(packageRequire.resolve('shadcn'))
const deepmerge = shadcnRequire('deepmerge')

test('deepmerge preserves an own __proto__ target key without changing the output prototype', () => {
  const target = JSON.parse('{"__proto__":{"targetFlag":true}}')
  const result = deepmerge(target, { safe: true })

  assert.equal(Object.getPrototypeOf(result), Object.prototype)
  assert.equal(Object.hasOwn(result, '__proto__'), true)
  assert.deepEqual(result.__proto__, { targetFlag: true })
  assert.equal(({}).targetFlag, undefined)
})

test('deepmerge ignores an own __proto__ source key without polluting objects', () => {
  const source = JSON.parse('{"__proto__":{"sourceFlag":true}}')
  const result = deepmerge({}, source)

  assert.equal(Object.getPrototypeOf(result), Object.prototype)
  assert.equal(Object.hasOwn(result, '__proto__'), false)
  assert.equal(({}).sourceFlag, undefined)
})

test('deepmerge keeps own constructor data without changing Object.prototype', () => {
  const target = JSON.parse('{"constructor":{"prototype":{"targetFlag":true}}}')
  const result = deepmerge(target, {})

  assert.equal(Object.getPrototypeOf(result), Object.prototype)
  assert.equal(Object.hasOwn(result, 'constructor'), true)
  assert.deepEqual(result.constructor.prototype, { targetFlag: true })
  assert.equal(({}).targetFlag, undefined)
})
