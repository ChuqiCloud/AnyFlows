import assert from 'node:assert/strict'
import test from 'node:test'

import {
  findSidebarFirstHref,
  findSidebarKeyByPath,
  findSidebarParentKeysByPath,
  findSidebarSectionKeyByPath,
} from '../src/shared/app-shell/sidebar/tree.ts'
import {
  adminWorkspaceItems,
  systemNavigationGroups,
  systemPinnedItems,
  systemSectionItems,
  userWorkspaceItems,
} from '../src/components/layout/app-navigation-model.ts'

const items = [
  { key: 'workspace', title: 'Workspace', children: [{ key: 'overview', title: 'Overview', href: '/console' }] },
  {
    key: 'system', title: 'System', children: [
      { key: 'empty', title: 'Empty', href: '#' },
      {
        key: 'gateway', title: 'Gateway', children: [
          { key: 'channels', title: 'Channels', href: '/console/channels' },
          { key: 'channel-detail', title: 'Channel', href: '/console/channels/7' },
        ],
      },
      { key: 'platform', title: 'Platform', children: [{ key: 'about', title: 'About', href: '/console/system-settings/about' }] },
    ],
  },
]

test('the console root never steals a more specific route from another section', () => {
  assert.equal(findSidebarKeyByPath(items, '/console'), 'overview')
  assert.equal(findSidebarKeyByPath(items, '/console/channels'), 'channels')
  assert.equal(findSidebarKeyByPath(items, '/console/channels/7'), 'channel-detail')
  assert.equal(findSidebarKeyByPath(items, '/console/channels/7/models'), 'channel-detail')
  assert.equal(findSidebarSectionKeyByPath(items, '/console/channels/7/models'), 'system')
  assert.equal(findSidebarKeyByPath(items, '/console/channels-other'), 'overview')
})

test('route expansion follows every parent and switches to the destination group', () => {
  assert.deepEqual(findSidebarParentKeysByPath(items, '/console/channels/7'), ['system', 'gateway'])
  assert.deepEqual(findSidebarParentKeysByPath(items, '/console/system-settings/about'), ['system', 'platform'])
  assert.deepEqual(findSidebarParentKeysByPath(items, '/console'), ['workspace'])
  assert.deepEqual(findSidebarParentKeysByPath(items, '/missing'), [])
  assert.deepEqual(findSidebarParentKeysByPath([], '/console'), [])
  assert.equal(findSidebarSectionKeyByPath(items, '/missing'), 'workspace')
  assert.equal(findSidebarSectionKeyByPath([], '/missing'), null)
})

test('section switching descends through groups and ignores non-navigable placeholders', () => {
  assert.equal(findSidebarFirstHref(items, 'system'), '/console/channels')
  assert.equal(findSidebarFirstHref(items, 'workspace'), '/console')
  assert.equal(findSidebarFirstHref(items, 'missing'), null)
  assert.equal(findSidebarFirstHref([{ key: 'empty', title: 'Empty', href: '#', children: [] }], 'empty'), null)
  assert.equal(findSidebarFirstHref([], 'system'), null)
})

test('grouped system navigation retains every existing administrator entry exactly once', () => {
  const grouped = [...systemPinnedItems, ...Object.values(systemNavigationGroups).flat()]
  assert.equal(new Set(grouped).size, grouped.length)
  assert.deepEqual([...grouped].sort(), [...systemSectionItems].sort())
  for (const key of systemSectionItems) {
    assert.ok(!userWorkspaceItems.includes(key), key)
  }
  assert.ok(adminWorkspaceItems.includes('overview'))
  assert.ok(userWorkspaceItems.includes('overview'))
  assert.ok(userWorkspaceItems.includes('apiKeys'))
  assert.ok(systemPinnedItems.includes('keys'))
})
