import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))

// The two frontends may evolve their pages and feature implementations independently.
// Only generated artifacts that define shared API and capability contracts must match.
const sharedContractPaths = [
  'openapi/openapi.json',
  'src/lib/api/generated/types.gen.ts',
  'src/lib/api/generated/sdk.gen.ts',
  'src/features/channels/channel-capabilities.gen.ts',
]

for (const path of sharedContractPaths) {
  const read = (frontend) => readFileSync(resolve(root, frontend, path), 'utf8').replaceAll('\r\n', '\n')
  assert.equal(read('web-next'), read('web'), `Shared frontend contract artifact drifted: ${path}`)
}
console.log('Shared frontend contract artifacts are synchronized.')
