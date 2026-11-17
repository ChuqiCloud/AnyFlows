import { Chip } from '@heroui/react'

type Tone = 'neutral' | 'success' | 'warning' | 'danger'
export type RenderablePluginUiBlock =
  | { type: 'text'; text: string }
  | { type: 'badge'; label: string; tone: Tone }
  | { type: 'metric'; label: string; value: string }
  | { type: 'table'; columns: string[]; rows: string[][] }
  | { type: 'list'; items: string[] }
  | { type: 'notice'; title: string; description?: string; tone: Tone }
  | { type: 'progress'; label: string; percent: number }
  | { type: 'info_card'; title: string; description?: string }
type View = { id: string; blocks: RenderablePluginUiBlock[] }

const SLOTS = new Set(['plugin_management', 'channel_monitor', 'channel_model_mapping', 'merchant_service', 'external_account'])
const ID_PATTERN = /^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$/u
const TEXT_LINK_PATTERN = /(?:https?:\/\/|javascript:|data:|mailto:|www\.)/iu
const TEXT_CONTROL_PATTERN = /[\u0000-\u001f\u007f<>]/u

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === 'object' && !Array.isArray(value))
}

function hasOnlyKeys(value: Record<string, unknown>, keys: string[]) {
  return Object.keys(value).every((key) => keys.includes(key))
}

export function localizedPluginUiText(value: unknown, locale: string): string | undefined {
  if (!isRecord(value) || !hasOnlyKeys(value, ['en', 'zh'])) return undefined
  const texts = Object.values(value)
  if (texts.length < 1 || texts.length > 2 || texts.some((text) =>
    typeof text !== 'string'
      || !text.trim()
      || text.length > 512
      || TEXT_CONTROL_PATTERN.test(text)
      || TEXT_LINK_PATTERN.test(text),
  )) return undefined
  const preferred = locale.toLowerCase().startsWith('zh') ? value.zh : value.en
  const fallback = value.en ?? value.zh
  return typeof preferred === 'string' ? preferred : typeof fallback === 'string' ? fallback : undefined
}

function parseBlock(value: unknown, locale: string): RenderablePluginUiBlock | undefined {
  if (!isRecord(value) || typeof value.type !== 'string') return undefined
  if (value.type === 'text' && hasOnlyKeys(value, ['type', 'text'])) {
    const text = localizedPluginUiText(value.text, locale)
    return text ? { type: 'text', text } : undefined
  }
  if (value.type === 'badge' && hasOnlyKeys(value, ['type', 'label', 'tone'])) {
    const label = localizedPluginUiText(value.label, locale)
    return label && ['neutral', 'success', 'warning', 'danger'].includes(String(value.tone))
      ? { type: 'badge', label, tone: value.tone as Tone }
      : undefined
  }
  if (value.type === 'metric' && hasOnlyKeys(value, ['type', 'label', 'value'])) {
    const label = localizedPluginUiText(value.label, locale)
    const metric = localizedPluginUiText(value.value, locale)
    return label && metric ? { type: 'metric', label, value: metric } : undefined
  }
  if (value.type === 'table' && hasOnlyKeys(value, ['type', 'columns', 'rows'])) {
    if (!Array.isArray(value.columns) || !Array.isArray(value.rows)
      || value.columns.length < 1 || value.columns.length > 8 || value.rows.length > 16) return undefined
    const columns = value.columns.map((column) => localizedPluginUiText(column, locale))
    const rows = value.rows.map((row) => Array.isArray(row)
      ? row.map((cell) => localizedPluginUiText(cell, locale))
      : null)
    if (columns.some((column) => !column)
      || rows.some((row) => !row || row.length !== columns.length || row.some((cell) => !cell))) return undefined
    return {
      type: 'table',
      columns: columns as string[],
      rows: rows as string[][],
    }
  }
  if (value.type === 'list' && hasOnlyKeys(value, ['type', 'items'])) {
    if (!Array.isArray(value.items) || value.items.length < 1 || value.items.length > 16) return undefined
    const items = value.items.map((item) => localizedPluginUiText(item, locale))
    if (items.some((item) => !item)) return undefined
    return { type: 'list', items: items as string[] }
  }
  if (value.type === 'notice' && hasOnlyKeys(value, ['type', 'title', 'description', 'tone'])) {
    const title = localizedPluginUiText(value.title, locale)
    const description = value.description === undefined ? undefined : localizedPluginUiText(value.description, locale)
    return title
      && (value.description === undefined || description)
      && ['neutral', 'success', 'warning', 'danger'].includes(String(value.tone))
      ? { type: 'notice', title, ...(description ? { description } : {}), tone: value.tone as Tone }
      : undefined
  }
  if (value.type === 'progress' && hasOnlyKeys(value, ['type', 'label', 'percent'])) {
    const label = localizedPluginUiText(value.label, locale)
    return label && typeof value.percent === 'number' && Number.isInteger(value.percent)
      && value.percent >= 0 && value.percent <= 100
      ? { type: 'progress', label, percent: value.percent }
      : undefined
  }
  if (value.type === 'info_card' && hasOnlyKeys(value, ['type', 'title', 'description'])) {
    const title = localizedPluginUiText(value.title, locale)
    const description = value.description === undefined ? undefined : localizedPluginUiText(value.description, locale)
    return title && (value.description === undefined || description)
      ? { type: 'info_card', title, ...(description ? { description } : {}) }
      : undefined
  }
  return undefined
}

function parseViews(manifest: unknown, slot: string, locale: string): View[] {
  if (!SLOTS.has(slot) || !isRecord(manifest) || !Array.isArray(manifest.uiViews)
    || manifest.uiViews.length > 8) return []
  const allViewsJson = JSON.stringify(manifest.uiViews)
  if (!allViewsJson || new TextEncoder().encode(allViewsJson).length > 32 * 1024) return []
  const ids = new Set<string>()
  if (Array.isArray(manifest.uiContributions)) {
    for (const contribution of manifest.uiContributions) {
      if (isRecord(contribution) && typeof contribution.id === 'string') ids.add(contribution.id)
    }
  }
  const views: View[] = []
  for (const candidate of manifest.uiViews) {
    if (!isRecord(candidate)
      || !hasOnlyKeys(candidate, ['id', 'schemaVersion', 'slot', 'blocks'])
      || typeof candidate.id !== 'string'
      || !ID_PATTERN.test(candidate.id)
      || ids.has(candidate.id)
      || candidate.schemaVersion !== 1
      || candidate.slot !== slot
      || !Array.isArray(candidate.blocks)
      || candidate.blocks.length < 1
      || candidate.blocks.length > 24) continue
    const candidateJson = JSON.stringify(candidate)
    if (!candidateJson || new TextEncoder().encode(candidateJson).length > 8 * 1024) continue
    const blocks = candidate.blocks.map((block) => parseBlock(block, locale))
    if (blocks.some((block) => !block)) continue
    ids.add(candidate.id)
    views.push({ id: candidate.id, blocks: blocks as RenderablePluginUiBlock[] })
  }
  return views
}

/** Parse a capability result as bounded host-rendered blocks; arbitrary markup is never accepted. */
export function parsePluginUiBlocks(value: unknown, locale: string): RenderablePluginUiBlock[] | undefined {
  if (!isRecord(value) || !hasOnlyKeys(value, ['blocks']) || !Array.isArray(value.blocks)
    || value.blocks.length < 1 || value.blocks.length > 24) return undefined
  const encoded = JSON.stringify(value.blocks)
  if (!encoded || new TextEncoder().encode(encoded).length > 8 * 1024) return undefined
  const blocks = value.blocks.map((block) => parseBlock(block, locale))
  return blocks.every(Boolean) ? blocks as RenderablePluginUiBlock[] : undefined
}

export function hasRenderablePluginUiViews(manifest: unknown, slot: string, locale: string) {
  return parseViews(manifest, slot, locale).length > 0
}

function RenderBlock({ block }: { block: RenderablePluginUiBlock }) {
  if (block.type === 'text') return <p className="whitespace-pre-wrap text-sm text-muted-foreground">{block.text}</p>
  if (block.type === 'badge') {
    const color: 'default' | 'success' | 'warning' | 'danger' = block.tone === 'neutral' ? 'default' : block.tone
    return <Chip size="sm" color={color} variant="flat">{block.label}</Chip>
  }
  if (block.type === 'metric') return <div className="flex flex-wrap items-baseline justify-between gap-2"><dt className="text-xs text-muted-foreground">{block.label}</dt><dd className="m-0 break-words text-sm font-semibold">{block.value}</dd></div>
  if (block.type === 'table') return <div className="overflow-x-auto"><table className="w-full border-collapse text-left text-xs"><thead><tr>{block.columns.map((column, index) => <th key={index} scope="col" className="border-b border-[var(--hairline)] px-2 py-1.5 font-medium text-muted-foreground">{column}</th>)}</tr></thead><tbody>{block.rows.map((row, rowIndex) => <tr key={rowIndex}>{row.map((cell, cellIndex) => <td key={cellIndex} className="border-b border-[var(--hairline)] px-2 py-1.5">{cell}</td>)}</tr>)}</tbody></table></div>
  if (block.type === 'list') return <ul className="list-disc space-y-1 pl-5 text-sm text-muted-foreground">{block.items.map((item, index) => <li key={index}>{item}</li>)}</ul>
  if (block.type === 'notice') {
    const toneClass: Record<Tone, string> = {
      neutral: 'border-[var(--hairline)] bg-content1/40',
      success: 'border-success/30 bg-success/5',
      warning: 'border-warning/30 bg-warning/5',
      danger: 'border-danger/30 bg-danger/5',
    }
    return <aside className={`rounded-lg border p-3 ${toneClass[block.tone]}`}><h3 className="text-sm font-medium">{block.title}</h3>{block.description ? <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">{block.description}</p> : null}</aside>
  }
  if (block.type === 'progress') return <div className="space-y-1"><div className="flex justify-between gap-2 text-xs"><span className="text-muted-foreground">{block.label}</span><span>{block.percent}%</span></div><progress className="h-2 w-full accent-primary" max={100} value={block.percent} aria-label={block.label} /></div>
  return <div className="rounded-lg border border-[var(--hairline)] bg-content1/40 p-3"><h3 className="text-sm font-medium">{block.title}</h3>{block.description ? <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">{block.description}</p> : null}</div>
}

export function PluginUiBlocksRenderer({ blocks }: { blocks: RenderablePluginUiBlock[] }) {
  return <div className="space-y-3" data-plugin-ui-blocks>{blocks.map((block, index) => <RenderBlock key={index} block={block} />)}</div>
}

/** Render static manifest views with host-owned components and text-only values. */
export function PluginUiViewsRenderer({ manifest, slot, locale }: { manifest: unknown; slot: string; locale: string }) {
  const views = parseViews(manifest, slot, locale)
  if (!views.length) return null
  return <div className="space-y-3" data-plugin-ui-views={slot}>{views.map((view) => <section key={view.id} className="space-y-2 rounded-lg border border-[var(--hairline)] bg-content1/40 p-3" data-plugin-ui-view={view.id}>{view.blocks.map((block, index) => <RenderBlock key={`${view.id}:${index}`} block={block} />)}</section>)}</div>
}
