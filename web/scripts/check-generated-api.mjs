import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

import { normalizeGeneratedApi } from './normalize-generated-api.mjs'

const webRoot = resolve(fileURLToPath(new URL('..', import.meta.url)))
const generatedRoot = join(webRoot, 'src', 'lib', 'api', 'generated')

function snapshotDirectory(root) {
  const files = new Map()

  if (!existsSync(root)) {
    return files
  }

  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) {
        visit(path)
        continue
      }
      files.set(relative(root, path), readFileSync(path, 'utf8'))
    }
  }

  visit(root)
  return files
}

const before = snapshotDirectory(generatedRoot)
const command = process.platform === 'win32' ? process.env.ComSpec ?? 'cmd.exe' : 'openapi-ts'
const args =
  process.platform === 'win32'
    ? ['/d', '/s', '/c', 'openapi-ts --no-log-file']
    : ['--no-log-file']
const result = spawnSync(command, args, {
  cwd: webRoot,
  encoding: 'utf8',
  stdio: 'inherit',
})

if (result.error) {
  console.error(`无法启动 OpenAPI 生成器：${result.error.message}`)
  process.exit(1)
}

if (result.status !== 0) {
  process.exit(result.status ?? 1)
}

normalizeGeneratedApi(generatedRoot)

const after = snapshotDirectory(generatedRoot)
const changed = new Set([...before.keys(), ...after.keys()])
const staleFiles = [...changed].filter((path) => before.get(path) !== after.get(path))

if (staleFiles.length > 0) {
  console.error(`OpenAPI 生成文件已过期，请先运行 pnpm api:generate：${staleFiles.join('、')}`)
  process.exit(1)
}
