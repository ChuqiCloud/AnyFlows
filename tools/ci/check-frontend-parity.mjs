import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)))
// Shared behavior and contracts must stay synchronized; presentation intentionally differs.
for (const path of [
  'openapi/openapi.json',
  'src/lib/api/generated/types.gen.ts',
  'src/lib/api/generated/sdk.gen.ts',
  'src/components/brand/provider-catalog.ts',
  'src/components/brand/model-logos.ts',
  'src/features/account-verification/alipay-authorization.ts',
  'src/features/channels/channel-form-model.ts',
  'src/features/usage-logs/usage-log-model.ts',
  'src/features/site-settings/frontend-template-model.ts',
  'src/features/site-settings/frontend-template-api.ts',
  'src/features/dashboard/dashboard-sla-api.ts',
  'src/features/dashboard/dashboard-sla-model.ts',
]) {
  const read = (frontend) => readFileSync(resolve(root, frontend, path), 'utf8').replaceAll('\r\n', '\n')
  assert.equal(read('web-next'), read('web'), `Shared frontend contract or behavior drifted: ${path}`)
}
console.log('Both frontend contracts and shared behavior are synchronized.')
