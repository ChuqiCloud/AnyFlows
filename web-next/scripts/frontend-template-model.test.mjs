import assert from 'node:assert/strict'
import test from 'node:test'
import { paginateTemplates } from '../src/features/site-settings/frontend-template-model.ts'

const templates = [
  { id: 'embedded', name: 'AnyFlows Classic', builtin: true },
  { id: 'embedded-next', name: 'AnyFlows Next', builtin: true },
  ...Array.from({ length: 21 }, (_, index) => ({ id: `theme-${index}`, name: `Theme ${index}`, builtin: false, author: 'Acme', description: 'Custom console' })),
]

test('pagination bounds metadata and preserves template order', () => {
  const first = paginateTemplates(templates, 'all', '', 1, 8)
  const last = paginateTemplates(templates, 'all', '', 3, 8)
  assert.equal(first.items.length, 8)
  assert.equal(first.items[0].id, 'embedded')
  assert.equal(first.pages, 3)
  assert.equal(last.items.length, 7)
  assert.equal(new Set([...first.items, ...paginateTemplates(templates, 'all', '', 2, 8).items, ...last.items].map((item) => item.id)).size, 23)
})

test('filters include authors, descriptions and case-insensitive identifiers', () => {
  assert.equal(paginateTemplates(templates, 'builtin', '', 1, 8).total, 2)
  assert.equal(paginateTemplates(templates, 'external', ' ACME ', 1, 8).total, 21)
  assert.equal(paginateTemplates(templates, 'all', 'custom console', 1, 8).total, 21)
  assert.equal(paginateTemplates(templates, 'all', 'EMBEDDED-NEXT', 1, 8).items[0].id, 'embedded-next')
})

test('rescan or narrower filters clamp stale pages and handle empty catalogs', () => {
  assert.equal(paginateTemplates(templates, 'builtin', '', 9, 8).page, 1)
  const empty = paginateTemplates([], 'all', '', -4, Number.NaN)
  assert.equal(empty.page, 1)
  assert.equal(empty.pages, 1)
  assert.equal(empty.total, 0)
  assert.deepEqual(empty.items, [])
  assert.equal(paginateTemplates(templates, 'all', '', 1, 999).pageSize, 24)
})
