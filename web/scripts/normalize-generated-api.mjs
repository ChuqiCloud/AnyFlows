import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import ts from 'typescript'

const header = '// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。'
const defaultRoot = resolve(
  fileURLToPath(new URL('../src/lib/api/generated', import.meta.url)),
)

// 去除第三方模板注释并统一中文文件头，保证生成结果可重复审查。
export function normalizeGeneratedApi(root = defaultRoot) {
  for (const path of listTypeScriptFiles(root)) {
    const source = ts.createSourceFile(
      path,
      readFileSync(path, 'utf8'),
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TS,
    )
    const printer = ts.createPrinter({
      newLine: ts.NewLineKind.LineFeed,
      removeComments: true,
    })
    const content = printer.printFile(source).trimStart()
    writeFileSync(path, `${header}\n\n${content}`, 'utf8')
  }
}

function listTypeScriptFiles(root) {
  const files = []

  function visit(directory) {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) {
        visit(path)
      } else if (entry.name.endsWith('.ts')) {
        files.push(path)
      }
    }
  }

  visit(root)
  return files
}

if (resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  normalizeGeneratedApi()
}
