import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import ts from 'typescript'

const header = '// 此文件由 @hey-api/openapi-ts 自动生成，请勿直接修改。'
const generatedTypeBoundary =
  '// @ts-ignore 自动生成的无限查询泛型由调用方补充分页配置。'
const defaultRoot = resolve(
  fileURLToPath(new URL('../src/lib/api/generated', import.meta.url)),
)

// 去除第三方模板注释并统一中文文件头，保证生成结果可重复审查。
export function normalizeGeneratedApi(root = defaultRoot) {
  for (const path of listTypeScriptFiles(root)) {
    const sourceText = readFileSync(path, 'utf8')
    const source = ts.createSourceFile(
      path,
      sourceText,
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TS,
    )
    preserveCompilerDirectives(source, sourceText)
    const printer = ts.createPrinter({
      newLine: ts.NewLineKind.LineFeed,
      removeComments: false,
    })
    const content = printer.printFile(source).trimStart()
    writeFileSync(path, `${header}\n\n${content}`, 'utf8')
  }
}

// 普通模板注释会增加审查噪音，但 TypeScript 编译指令属于代码语义，必须保留。
function preserveCompilerDirectives(source, content) {
  const preservedComments = new Set()

  function visit(node) {
    const comments = ts.getLeadingCommentRanges(content, node.getFullStart()) ?? []
    ts.setEmitFlags(
      node,
      ts.getEmitFlags(node) |
        ts.EmitFlags.NoLeadingComments |
        ts.EmitFlags.NoTrailingComments,
    )

    for (const comment of comments) {
      if (preservedComments.has(comment.pos)) {
        continue
      }
      const text = content.slice(comment.pos, comment.end)
      if (/^\/\/\s*@ts-(?:ignore|expect-error)\b/.test(text)) {
        preservedComments.add(comment.pos)
        ts.addSyntheticLeadingComment(
          node,
          ts.SyntaxKind.SingleLineCommentTrivia,
          generatedTypeBoundary.slice(2),
          true,
        )
      }
    }

    ts.forEachChild(node, visit)
  }

  visit(source)
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
