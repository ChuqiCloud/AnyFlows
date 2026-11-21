import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve, sep } from 'node:path'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'
import * as zod from 'zod'

// Extract factual catalog fields from a trusted LobeHub checkout at a known revision.
// Usage: node scripts/export-lobehub-catalog.mjs <lobehub-root> <commit-sha>
const [checkout, revision] = process.argv.slice(2)
if (!checkout || !/^[\da-f]{40}$/.test(revision ?? '')) throw new Error('Expected a LobeHub checkout and full commit SHA')
const source = resolve(checkout, 'packages/model-bank/src')
const modules = new Map()

function load(request, importer = resolve(source, 'entry.ts')) {
  if (request === 'zod') return zod
  if (request === '@lobechat/business-const') return { BRANDING_NAME: 'LobeHub', ENABLE_BUSINESS_FEATURES: true }
  if (!request.startsWith('.')) throw new Error(`Unexpected model-bank dependency: ${request}`)
  const base = resolve(dirname(importer), request)
  const file = [`${base}.ts`, resolve(base, 'index.ts')].find(existsSync)
  if (!file?.startsWith(source + sep)) throw new Error(`Missing model-bank module: ${request} from ${importer}`)
  if (modules.has(file)) return modules.get(file).exports

  const module = { exports: {} }
  modules.set(file, module)
  const { outputText } = ts.transpileModule(readFileSync(file, 'utf8'), {
    fileName: file,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  })
  runInNewContext(outputText, {
    exports: module.exports,
    module,
    require: (specifier) => load(specifier, file),
  }, { filename: file, timeout: 5000 })
  return module.exports
}

const { DEFAULT_MODEL_PROVIDER_LIST } = load('./modelProviders')
const { loadModels } = load('./aiModels')
const entries = await loadModels()
const providers = new Map(DEFAULT_MODEL_PROVIDER_LIST.map(({ id, name }) => [id, { id, name }]))
const models = new Map()
let sourceCount = 0

function modalities(values, fallback) {
  const supported = [...new Set((values ?? []).filter((value) => ['text', 'image', 'audio', 'video'].includes(value)))]
  return supported.length ? supported : fallback
}

function supportedIo(card) {
  const { abilities = {}, type } = card
  const input = type === 'asr'
    ? ['audio']
    : type === 'realtime'
      ? ['text', 'audio']
      : abilities.vision ? ['text', 'image'] : ['text']
  const output = {
    image: ['image'], text2music: ['audio'], realtime: ['text', 'audio'], tts: ['audio'], video: ['video'],
  }[type] ?? ['text']
  return [modalities(card.inputModalities, input), modalities(card.outputModalities, output)]
}

function text(value, limit) {
  let clipped = ''
  for (const character of value.trim().replace(/\p{Cc}/gu, ' ')) {
    if (Buffer.byteLength(clipped + character) > limit) break
    clipped += character
  }
  return clipped.trim()
}
const compare = (left, right) => left < right ? -1 : left > right ? 1 : 0
const directProviders = new Set([
  'ai21', 'anthropic', 'baichuan', 'bfl', 'cohere', 'deepseek', 'elevenlabs', 'fishaudio',
  'google', 'hunyuan', 'jina', 'longcat', 'meta', 'minimax', 'mistral', 'moonshot', 'openai',
  'qwen', 'spark', 'stepfun', 'wenxin', 'xai', 'xiaomimimo', 'zhipu',
])
const normalize = (value) => value.toLowerCase().replace(/[^a-z0-9]/g, '')
const organizationAliases = new Map([
  ['anthropic', 'anthropic'], ['blackforestlabs', 'bfl'], ['googledeepmind', 'google'],
  ['metaai', 'meta'], ['moonshotai', 'moonshot'], ['openaichatgpt', 'openai'],
  ['zai', 'zhipu'], ['zhipuai', 'zhipu'],
])

for (const card of entries) {
  const { id, providerId, type, abilities = {}, contextWindowTokens } = card
  if (!id || id.trim() !== id || Buffer.byteLength(id) > 256 || /[\u0000-\u0020\u007f]/.test(id)) {
    throw new Error(`Invalid model ID: ${JSON.stringify(id)}`)
  }
  if (!providers.has(providerId)) throw new Error(`Unknown provider ${providerId} for ${id}`)
  sourceCount++
  const [input, output] = supportedIo(card)
  const candidate = {
    id,
    name: text(card.displayName ?? id, 128) || text(id, 128),
    provider: providerId,
    input,
    output,
    type: type ?? 'chat',
    context: Number.isInteger(contextWindowTokens) && contextWindowTokens > 0 && contextWindowTokens <= 2147483647
      ? contextWindowTokens
      : null,
    reasoning: abilities.reasoning === true,
    tools: abilities.functionCall === true,
  }
  const candidates = models.get(id) ?? []
  candidates.push({ candidate, official: card.isOfficial === true, organization: card.organization })
  models.set(id, candidates)
}

function chooseCanonical(candidates) {
  const organizations = new Set(candidates.flatMap(({ organization }) => {
    if (typeof organization !== 'string') return []
    const key = normalize(organization)
    return key ? [organizationAliases.get(key) ?? key] : []
  }))
  const rank = ({ candidate: { provider }, official }) => {
    const attributed = organizations.has(normalize(provider)) || organizations.has(normalize(providers.get(provider).name))
    return (official ? 4 : 0) + (attributed ? 2 : 0) + (directProviders.has(provider) ? 1 : 0)
  }
  const [first] = candidates.sort((left, right) => (
    rank(right) - rank(left) || compare(left.candidate.provider, right.candidate.provider)
  ))
  return { ...first.candidate, providers: [...new Set(candidates.map(({ candidate }) => candidate.provider))].sort(compare) }
}

const snapshot = {
  revision,
  sourceCount,
  providers: [...providers.values()].sort((left, right) => compare(left.id, right.id)),
  models: [...models.values()].map(chooseCanonical).sort((left, right) => compare(left.id, right.id)),
}
const rows = (items) => items.map((item) => `    ${JSON.stringify(item)}`).join(',\n')
writeFileSync(new URL('../src/components/brand/lobehub-providers.ts', import.meta.url),
  `// Generated by scripts/export-lobehub-catalog.mjs from ${revision}.\nexport const lobeHubModelCount = ${snapshot.models.length}\nexport const lobeHubProviders = [\n${snapshot.providers.map((provider) => `  ${JSON.stringify(provider)},`).join('\n')}\n] as const\n`)
const output = `{\n  "revision": "${revision}",\n  "sourceCount": ${sourceCount},\n  "models": [\n${rows(snapshot.models)}\n  ]\n}\n`
writeFileSync(new URL('../src/features/model-management/lobehub-catalog.json', import.meta.url), output)
console.log(`${snapshot.providers.length} providers, ${models.size} unique model IDs, ${sourceCount} source rows`)
