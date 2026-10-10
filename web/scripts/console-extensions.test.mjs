import assert from 'node:assert/strict'
import test from 'node:test'

import { supportsExtensionRoute, validateExtensionRoutes } from '../src/extensions/model.ts'

const route = { path: '/console/extensions/sample', access: 'user', titleKey: 'sample:title', capabilities: ['directory', 'wallet'] }

test('extension routes stay inside their reserved console namespace', () => {
  assert.throws(() => validateExtensionRoutes([{ ...route, path: '/console/users' }]))
  assert.throws(() => validateExtensionRoutes([{ ...route, path: '/console/extensions/../users' }]))
  assert.throws(() => validateExtensionRoutes([{ ...route, path: '/console/extensions/sample?admin=true' }]))
  assert.throws(() => validateExtensionRoutes([route, route]))
  assert.throws(() => validateExtensionRoutes([{ ...route, capabilities: [] }]))
})

test('navigation requires every declared runtime capability', () => {
  assert.equal(supportsExtensionRoute(route, new Set(['directory'])), false)
  assert.equal(supportsExtensionRoute(route, new Set(['directory', 'wallet'])), true)
  assert.equal(validateExtensionRoutes([route])[0], route)
})
