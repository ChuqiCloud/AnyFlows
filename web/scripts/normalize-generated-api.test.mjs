import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { normalizeGeneratedApi } from './normalize-generated-api.mjs'

test('规范化生成物时保留 TypeScript 编译指令', () => {
  const root = mkdtempSync(join(tmpdir(), 'anyflows-generated-api-'))
  const path = join(root, 'query.gen.ts')

  try {
    writeFileSync(
      path,
      [
        '// Generated helper comment.',
        'export type ClientOptions = { baseUrl: `${string}://${string}` }',
        'export const options = infiniteQueryOptions(',
        '  // @ts-ignore',
        '  { queryKey: ["users"] },',
        ')',
      ].join('\n'),
      'utf8',
    )

    normalizeGeneratedApi(root)
    const normalized = readFileSync(path, 'utf8')

    assert.match(
      normalized,
      /@ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。/,
    )
    assert.doesNotMatch(normalized, /@ts-expect-error/)
    assert.doesNotMatch(normalized, /Generated helper comment/)
    assert.ok(normalized.includes('baseUrl: `${string}://${string}`'))
  } finally {
    rmSync(root, { force: true, recursive: true })
  }
})
