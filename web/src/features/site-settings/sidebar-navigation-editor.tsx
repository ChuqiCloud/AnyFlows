import { ArrowDown, ArrowUp, GripVertical, Plus, Trash2 } from 'lucide-react'
import { useState, type ComponentType } from 'react'

import type { SiteSidebarLink } from '@/lib/api/generated/types.gen'

type IconOption = { key: string; Icon: ComponentType<{ className?: string }> }
type Node = { link: SiteSidebarLink; children: Node[] }
type Position = { parent: number | null; index: number }

const fieldClass = 'h-9 min-w-0 w-full rounded-md border border-[var(--hairline)] bg-background px-3 text-sm outline-none focus:border-primary'
const emptyLink = (): SiteSidebarLink => ({ label: '', label_en: null, url: '', icon: 'link', kind: 'link', level: 1, style: 'default', audience: 'all' })

function toTree(links: SiteSidebarLink[]): Node[] {
  const roots: Node[] = []
  for (const item of links) {
    const link = { ...item, kind: item.kind ?? 'link' }
    if ((item.level ?? 1) > 1 && roots.length > 0) {
      const parent = roots.at(-1)!
      // Legacy parents were links; keep their destination when turning them into menus.
      if (parent.link.kind !== 'group' && parent.link.url) {
        parent.children.push({ link: { ...parent.link, level: 2 }, children: [] })
      }
      parent.link.kind = 'group'
      parent.link.url = ''
      parent.children.push({ link: { ...link, kind: 'link' }, children: [] })
    } else {
      roots.push({ link, children: [] })
    }
  }
  return roots
}

function flatten(nodes: Node[]): SiteSidebarLink[] {
  return nodes.flatMap(({ link, children }) => [
    { ...link, kind: children.length > 0 || link.kind === 'group' ? 'group' : 'link', level: 1, url: link.kind === 'group' ? '' : link.url },
    ...children.map((child) => ({ ...child.link, kind: 'link', level: 2 })),
  ])
}

export const normaliseSidebarLinks = (links: SiteSidebarLink[]): SiteSidebarLink[] => flatten(toTree(links))

export function SidebarNavigationEditor({ links, onChange, icons, en }: {
  links: SiteSidebarLink[]
  onChange: (links: SiteSidebarLink[]) => void
  icons: readonly IconOption[]
  en: boolean | undefined
}) {
  const [drag, setDrag] = useState<Position | null>(null)
  const words = en ? {
    addLink: 'Add link', addGroup: 'Add collapsible menu', link: 'Link', group: 'Collapsible menu',
    name: 'Name', english: 'English name (optional)', url: 'Site path / or HTTPS URL',
    visibility: 'Visible to', all: 'All users', admin: 'Admins only', icon: 'Icon',
    placement: 'Place in', top: 'Top level', remove: 'Remove', up: 'Move up', down: 'Move down',
    drag: 'Drag to reorder or place in a menu', drop: 'Drop here', empty: 'No sidebar links',
    emptyGroup: 'Add a link to this menu before saving',
  } : {
    addLink: '添加链接', addGroup: '添加折叠菜单', link: '直达链接', group: '折叠菜单',
    name: '菜单名称', english: '英文名称（可选）', url: '站内路径 / 或 HTTPS 地址',
    visibility: '可见范围', all: '所有用户', admin: '仅管理员', icon: '图标',
    placement: '归属菜单', top: '顶层', remove: '删除', up: '上移', down: '下移',
    drag: '拖动排序或移入菜单', drop: '放在这里', empty: '暂无侧栏入口',
    emptyGroup: '请先为此菜单添加链接',
  }
  const nodes = toTree(links)
  const commit = (next: Node[]) => onChange(flatten(next))
  const clone = () => toTree(links)
  const getList = (tree: Node[], parent: number | null) => parent === null ? tree : tree[parent].children
  const update = (position: Position, change: Partial<SiteSidebarLink>) => {
    const next = clone()
    const node = getList(next, position.parent)[position.index]
    node.link = { ...node.link, ...change }
    commit(next)
  }
  const relocate = (source: Position, target: Position) => {
    const next = clone()
    const sourceList = getList(next, source.parent)
    const [node] = sourceList.splice(source.index, 1)
    if (!node || (target.parent !== null && node.link.kind === 'group')) return
    let parent = target.parent
    if (source.parent === null && parent !== null && source.index < parent) parent--
    const targetList = getList(next, parent)
    const index = source.parent === parent && source.index < target.index ? target.index - 1 : target.index
    targetList.splice(index, 0, node)
    commit(next)
  }
  const changeKind = (position: Position, kind: 'link' | 'group') => {
    if (kind === 'group' && links.length >= 48) return
    const next = clone()
    const node = next[position.index]
    if (kind === 'group') {
      const previous = node.link
      node.link = { ...node.link, kind, url: '' }
      node.children = [{ link: { ...previous, kind: 'link', level: 2 }, children: [] }]
    } else {
      node.link = { ...node.link, kind, url: '' }
      next.splice(position.index + 1, 0, ...node.children)
      node.children = []
    }
    commit(next)
  }
  const remove = (position: Position) => {
    const next = clone()
    if (position.parent === null) {
      const [node] = next.splice(position.index, 1)
      if (node) next.splice(position.index, 0, ...node.children)
    } else next[position.parent].children.splice(position.index, 1)
    commit(next)
  }
  const dropZone = (target: Position) => <div
    className={`flex h-3 items-center rounded border border-dashed transition-all ${drag ? 'my-1 h-8 border-primary/60 bg-primary/5 text-primary' : 'border-transparent'}`}
    onDragOver={(event) => { if (drag && (target.parent === null || getList(nodes, drag.parent)[drag.index]?.link.kind !== 'group')) event.preventDefault() }}
    onDrop={(event) => { event.preventDefault(); if (drag) relocate(drag, target); setDrag(null) }}
  >{drag && <span className="px-3 text-xs">{words.drop}</span>}</div>
  const renderNode = (node: Node, position: Position) => {
    const { link } = node
    const list = getList(nodes, position.parent)
    const group = position.parent === null && link.kind === 'group'
    return <div key={position.index} className={`min-w-0 rounded-md border border-[var(--hairline)] bg-background/50 ${group ? 'border-l-2 border-l-primary/60' : ''}`}>
      <div className="flex min-w-0 items-start gap-2 p-3">
        <button type="button" draggable title={words.drag} aria-label={words.drag} className="mt-1 grid size-8 shrink-0 cursor-grab place-items-center rounded text-muted-foreground hover:bg-primary/10 hover:text-primary" onDragStart={(event) => { event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData('text/plain', `${position.parent ?? 'root'}:${position.index}`); setDrag(position) }} onDragEnd={() => setDrag(null)}><GripVertical className="size-4" /></button>
        <div className="min-w-0 flex-1 space-y-2">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-xs font-semibold text-foreground">{group ? words.group : words.link}</span>
            {position.parent === null && <select className="rounded border border-[var(--hairline)] bg-background px-2 py-1 text-xs" aria-label={words.placement} value={group ? 'group' : 'link'} onChange={(event) => changeKind(position, event.target.value as 'link' | 'group')}><option value="link">{words.link}</option><option value="group" disabled={!group && links.length >= 48}>{words.group}</option></select>}
            {!group && <select className="max-w-full rounded border border-[var(--hairline)] bg-background px-2 py-1 text-xs" aria-label={words.placement} value={position.parent ?? 'top'} onChange={(event) => relocate(position, { parent: event.target.value === 'top' ? null : Number(event.target.value), index: event.target.value === 'top' ? nodes.length : nodes[Number(event.target.value)].children.length })}><option value="top">{words.top}</option>{nodes.map((root, index) => root.link.kind === 'group' && (position.parent !== null || position.index !== index) ? <option key={index} value={index}>{root.link.label || words.group}</option> : null)}</select>}
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <input className={fieldClass} aria-label={words.name} placeholder={words.name} maxLength={64} value={link.label} onChange={(event) => update(position, { label: event.target.value })} />
            <input className={fieldClass} aria-label={words.english} placeholder={words.english} maxLength={64} value={link.label_en ?? ''} onChange={(event) => update(position, { label_en: event.target.value || null })} />
          </div>
          {!group && <input className={fieldClass} aria-label={words.url} placeholder={words.url} maxLength={2048} value={link.url} onChange={(event) => update(position, { url: event.target.value })} />}
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-muted-foreground">{words.visibility}<select className="rounded border border-[var(--hairline)] bg-background px-2 py-1.5 text-xs text-foreground" value={link.audience} onChange={(event) => update(position, { audience: event.target.value })}><option value="all">{words.all}</option><option value="admin">{words.admin}</option></select></label>
            <details className="relative"><summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground">{words.icon}</summary><div className="absolute z-20 mt-2 grid max-h-44 w-64 grid-cols-8 gap-1 overflow-y-auto rounded-md border border-[var(--hairline)] bg-background p-2 shadow-lg">{icons.map(({ key, Icon }) => <button key={key} type="button" title={key} aria-label={`${words.icon}: ${key}`} aria-pressed={link.icon === key} className={`grid size-7 place-items-center rounded ${link.icon === key ? 'bg-primary/15 text-primary' : 'hover:bg-primary/10'}`} onClick={(event) => { update(position, { icon: key }); event.currentTarget.closest('details')?.removeAttribute('open') }}><Icon className="size-4" /></button>)}</div></details>
          </div>
        </div>
        <div className="flex shrink-0 flex-col gap-1 sm:flex-row">
          <button type="button" title={words.up} aria-label={words.up} disabled={position.index === 0} className="grid size-8 place-items-center rounded hover:bg-primary/10 disabled:opacity-30" onClick={() => relocate(position, { parent: position.parent, index: position.index - 1 })}><ArrowUp className="size-4" /></button>
          <button type="button" title={words.down} aria-label={words.down} disabled={position.index === list.length - 1} className="grid size-8 place-items-center rounded hover:bg-primary/10 disabled:opacity-30" onClick={() => relocate(position, { parent: position.parent, index: position.index + 2 })}><ArrowDown className="size-4" /></button>
          <button type="button" title={words.remove} aria-label={words.remove} className="grid size-8 place-items-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive" onClick={() => remove(position)}><Trash2 className="size-4" /></button>
        </div>
      </div>
      {group && <div className="border-t border-[var(--hairline)] bg-surface-1/35 py-1 pl-4 pr-2 sm:pl-10">
        {node.children.length === 0 && <p className="px-2 py-2 text-xs text-destructive">{words.emptyGroup}</p>}
        {node.children.map((child, index) => <div key={index}>{dropZone({ parent: position.index, index })}{renderNode(child, { parent: position.index, index })}</div>)}
        {dropZone({ parent: position.index, index: node.children.length })}
        <button type="button" className="my-1 flex items-center gap-1 rounded px-2 py-1 text-xs text-primary hover:bg-primary/10 disabled:opacity-40" disabled={links.length >= 48} onClick={() => { const next = clone(); next[position.index].children.push({ link: emptyLink(), children: [] }); commit(next) }}><Plus className="size-3.5" />{words.addLink}</button>
      </div>}
    </div>
  }
  return <div className="space-y-1">
    <div className="mb-3 flex flex-wrap justify-end gap-2">
      <button type="button" className="flex h-8 items-center gap-1 rounded-md border border-[var(--hairline)] px-3 text-xs font-medium hover:bg-primary/5 disabled:opacity-40" disabled={links.length >= 48} onClick={() => commit([...clone(), { link: emptyLink(), children: [] }])}><Plus className="size-4" />{words.addLink}</button>
      <button type="button" className="flex h-8 items-center gap-1 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:opacity-90 disabled:opacity-40" disabled={links.length >= 47} onClick={() => commit([...clone(), { link: { ...emptyLink(), kind: 'group' }, children: [{ link: emptyLink(), children: [] }] }])}><Plus className="size-4" />{words.addGroup}</button>
    </div>
    {nodes.map((node, index) => <div key={index}>{dropZone({ parent: null, index })}{renderNode(node, { parent: null, index })}</div>)}
    {dropZone({ parent: null, index: nodes.length })}
    {nodes.length === 0 && <p className="py-4 text-center text-xs text-muted-foreground">{words.empty}</p>}
  </div>
}
