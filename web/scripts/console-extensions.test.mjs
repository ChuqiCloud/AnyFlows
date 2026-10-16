import assert from 'node:assert/strict'
import test from 'node:test'

import { groupExtensionNavigation, supportsExtensionRoute, validateExtensionRoutes } from '../src/extensions/model.ts'

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

test('navigation metadata rejects ambiguous groups and unsupported presentation', () => {
  const navigation = { group: { id: 'sample-workspace', titleKey: 'sample:workspace' }, icon: 'building', order: 10 }
  const grouped = { ...route, navigation }
  assert.equal(validateExtensionRoutes([grouped])[0], grouped)
  for (const group of [{ id: '', titleKey: 'title' }, { id: 'extensions', titleKey: 'title' }, { id: 'workspace', titleKey: ' ' }]) {
    assert.throws(() => validateExtensionRoutes([{ ...route, navigation: { group } }]))
  }
  assert.throws(() => validateExtensionRoutes([grouped, { ...grouped, path: '/console/extensions/second', navigation: { ...navigation, group: { ...navigation.group, titleKey: 'other:title' } } }]))
  assert.throws(() => validateExtensionRoutes([{ ...grouped, navigation: { ...navigation, icon: 'unknown' } }]))
  assert.throws(() => validateExtensionRoutes([{ ...grouped, navigation: { ...navigation, order: NaN } }]))
})

test('filtered navigation groups preserve stable order without mutating the registry', () => {
  const grouped = (path, access, id, order, capabilities = ['directory']) => ({ ...route, path: '/console/extensions/' + path, access, capabilities, navigation: { group: { id, titleKey: 'sample:' + id }, order } })
  const routes = [grouped('admin', 'admin', 'sample-admin', 30), grouped('catalog', 'user', 'sample-workspace', 20), grouped('home', 'user', 'sample-workspace', 10), grouped('hidden', 'user', 'sample-hidden', 5, ['unavailable']), route]
  const visible = routes.filter((item) => item.access === 'user' && supportsExtensionRoute(item, new Set(['directory', 'wallet'])))
  const groups = groupExtensionNavigation(visible)
  assert.deepEqual(groups.map((group) => group.id), ['extensions', 'sample-workspace'])
  assert.deepEqual(groups[1].routes.map((item) => item.path), ['/console/extensions/home', '/console/extensions/catalog'])
  assert.equal(routes[0].access, 'admin')
  assert.deepEqual(groupExtensionNavigation([]), [])
  assert.deepEqual(groupExtensionNavigation([route, { ...route, path: '/console/extensions/second' }])[0].routes.map((item) => item.path), [route.path, '/console/extensions/second'])
})

test('account verification destinations are unique user routes', () => {
  const destination = { ...route, entryPoint: 'account-verification' }
  assert.equal(validateExtensionRoutes([destination])[0], destination)
  assert.throws(() => validateExtensionRoutes([{ ...destination, access: 'admin' }]))
  assert.throws(() => validateExtensionRoutes([{ ...destination, entryPoint: 'unknown' }]))
  assert.throws(() => validateExtensionRoutes([destination, { ...destination, path: '/console/extensions/second' }]))
})
