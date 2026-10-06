import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const packageJsonPath = fileURLToPath(new URL('../package.json', import.meta.url))
const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'))

test('质量门禁脚本不递归调用包管理器', () => {
  const nestedPackageManager =
    /(?:^|[;&|])\s*(?:corepack\s+)?(?:pnpm|npm|yarn|bun)\b/

  for (const scriptName of ['lint', 'check']) {
    assert.doesNotMatch(
      packageJson.scripts[scriptName],
      nestedPackageManager,
      `${scriptName} 必须直接执行检查，避免 CNB 环境缺少包管理器命令`,
    )
  }
})
