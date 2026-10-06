import { ArrowDown, ArrowUp, Bell, BookOpen, Bookmark, Bot, BriefcaseBusiness, Building2, CalendarRange, ChartNoAxesCombined, CircleHelp, ClipboardCheck, Cloud, Code2, Command, Cpu, CreditCard, Database, FileText, Folder, Globe2, GraduationCap, Gauge, Headphones, Heart, Home, Info, KeyRound, LayoutDashboard, Link2, ListChecks, LockKeyhole, Mail, Map, Megaphone, MessageSquareText, Monitor, Newspaper, PanelsTopLeft, Phone, Plus, Rocket, Search, Server, Settings2, ShieldCheck, ShoppingBag, Sparkles, Star, Terminal, Trash2, UserRound, UsersRound, Workflow, Wrench, Zap } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import type { AdminSiteSettings, SiteNavigation, SiteNavigationGroup, SiteNavigationLink } from '@/lib/api/generated/types.gen'
import { siteSettingsErrorCode, useUpdateSiteNavigation } from './site-settings-api'
import { normaliseSidebarLinks, SidebarNavigationEditor } from './sidebar-navigation-editor'

const iconOptions = [
  { key: 'link', Icon: Link2 }, { key: 'globe', Icon: Globe2 }, { key: 'book', Icon: BookOpen },
  { key: 'sparkles', Icon: Sparkles }, { key: 'building', Icon: Building2 },
  { key: 'message', Icon: MessageSquareText }, { key: 'headphones', Icon: Headphones },
  { key: 'shield', Icon: ShieldCheck }, { key: 'chart', Icon: ChartNoAxesCombined },
  { key: 'home', Icon: Home }, { key: 'dashboard', Icon: LayoutDashboard }, { key: 'settings', Icon: Settings2 },
  { key: 'users', Icon: UsersRound }, { key: 'user', Icon: UserRound }, { key: 'briefcase', Icon: BriefcaseBusiness },
  { key: 'calendar', Icon: CalendarRange }, { key: 'card', Icon: CreditCard }, { key: 'key', Icon: KeyRound },
  { key: 'lock', Icon: LockKeyhole }, { key: 'file', Icon: FileText }, { key: 'folder', Icon: Folder },
  { key: 'help', Icon: CircleHelp }, { key: 'info', Icon: Info }, { key: 'bell', Icon: Bell },
  { key: 'mail', Icon: Mail }, { key: 'phone', Icon: Phone }, { key: 'map', Icon: Map },
  { key: 'database', Icon: Database }, { key: 'server', Icon: Server }, { key: 'code', Icon: Code2 },
  { key: 'terminal', Icon: Terminal }, { key: 'bot', Icon: Bot }, { key: 'cpu', Icon: Cpu },
  { key: 'workflow', Icon: Workflow }, { key: 'gauge', Icon: Gauge }, { key: 'rocket', Icon: Rocket },
  { key: 'megaphone', Icon: Megaphone }, { key: 'shopping', Icon: ShoppingBag }, { key: 'monitor', Icon: Monitor },
  { key: 'cloud', Icon: Cloud }, { key: 'bookmark', Icon: Bookmark }, { key: 'graduation', Icon: GraduationCap },
  { key: 'newspaper', Icon: Newspaper }, { key: 'clipboard', Icon: ClipboardCheck }, { key: 'list', Icon: ListChecks },
  { key: 'wrench', Icon: Wrench }, { key: 'search', Icon: Search }, { key: 'star', Icon: Star },
  { key: 'heart', Icon: Heart }, { key: 'zap', Icon: Zap }, { key: 'command', Icon: Command }, { key: 'panels', Icon: PanelsTopLeft },
] as const
const emptyLink = (): SiteNavigationLink => ({ label: '', label_en: null, url: '' })
const normaliseNavigation = (navigation: SiteNavigation): SiteNavigation => ({
  ...navigation,
  sidebar_links: normaliseSidebarLinks(navigation.sidebar_links),
})
const fieldClass = 'h-9 min-w-0 w-full rounded-md border border-[var(--hairline)] bg-background px-3 text-sm outline-none focus:border-primary'

function move<T>(items: T[], index: number, offset: number): T[] {
  const target = index + offset
  if (target < 0 || target >= items.length) return items
  const next = [...items]
  ;[next[index], next[target]] = [next[target], next[index]]
  return next
}

export function SiteNavigationPanel({ settings }: { settings: AdminSiteSettings }) {
  const { i18n } = useTranslation()
  const en = i18n.resolvedLanguage?.startsWith('en')
  const text = en ? {
    title: 'Navigation', description: 'Add links to the public header, footer, and console sidebar.',
    header: 'Header links', footer: 'Footer groups', sidebar: 'Console sidebar',
    addLink: 'Add link', addGroup: 'Add group', label: 'Label', english: 'English label (optional)',
    url: 'Site path / or HTTPS URL', groupTitle: 'Group title', groupEnglish: 'English title (optional)',
    icon: 'Icon', audience: 'Visibility', all: 'All users', admin: 'Admins only',
    save: 'Save navigation', saving: 'Saving...',
    empty: 'No links yet.', invalid: 'Check names and URLs. Links must start with / or https://; every collapsible menu needs at least one valid link.',
    conflict: 'Settings changed. Refresh the page and review your draft.', failed: 'Could not save navigation.',
    up: 'Move up', down: 'Move down', remove: 'Remove',
  } : {
    title: '自定义导航', description: '配置公开顶栏、页脚和控制中心侧栏的附加入口。',
    header: '顶栏链接', footer: '页脚分组', sidebar: '控制中心侧栏',
    addLink: '添加链接', addGroup: '添加分组', label: '名称', english: '英文名称（可选）',
    url: '站内路径 / 或 HTTPS 地址', groupTitle: '分组名称', groupEnglish: '英文分组名（可选）',
    icon: '图标', audience: '可见范围', all: '所有用户', admin: '仅管理员',
    save: '保存导航', saving: '正在保存...',
    empty: '暂无入口。', invalid: '请检查名称和地址。链接须以 / 或 https:// 开头；折叠菜单至少需要一条有效链接。',
    conflict: '站点设置已更新，请刷新后核对草稿。', failed: '导航保存失败。',
    up: '上移', down: '下移', remove: '删除',
  }
  const [draft, setDraft] = useState<SiteNavigation>(() => normaliseNavigation(settings.navigation))
  const [localError, setLocalError] = useState(false)
  const mutation = useUpdateSiteNavigation()
  useEffect(() => { setDraft(normaliseNavigation(settings.navigation)); setLocalError(false) }, [settings.navigation])

  const updateHeader = (index: number, link: SiteNavigationLink) => setDraft((current) => ({ ...current, header_links: current.header_links.map((item, position) => position === index ? link : item) }))
  const updateGroup = (index: number, group: SiteNavigationGroup) => setDraft((current) => ({ ...current, footer_groups: current.footer_groups.map((item, position) => position === index ? group : item) }))
  const validLink = (link: SiteNavigationLink) => {
    const url = link.url.trim()
    if (!link.label.trim() || !url || url !== link.url) return false
    if (url.startsWith('/')) return !url.startsWith('//') && !/[?#\\]/.test(url)
    try { const parsed = new URL(url); return parsed.protocol === 'https:' && !parsed.username && !parsed.password } catch { return false }
  }
  const save = async () => {
    const sidebarValid = draft.sidebar_links.every((link, index) => {
      if (!link.label.trim()) return false
      if (link.kind === 'group') return link.level === 1 && !link.url && draft.sidebar_links[index + 1]?.level === 2
      return validLink(link) && ((link.level ?? 1) === 1 || (link.level === 2 && draft.sidebar_links.slice(0, index).reverse().find((parent) => parent.level === 1)?.kind === 'group'))
    })
    if (!sidebarValid || draft.header_links.some((link) => !validLink(link)) || draft.footer_groups.some((group) => !group.title.trim() || group.links.length === 0 || group.links.some((link) => !validLink(link)))) {
      setLocalError(true)
      return
    }
    setLocalError(false)
    try { await mutation.mutateAsync({ navigation: draft, expected_version: settings.version }) } catch { /* 草稿保留 */ }
  }
  const actions = (index: number, count: number, onMove: (offset: number) => void, onRemove: () => void) => <div className="flex shrink-0 gap-1"><Button type="button" variant="ghost" size="icon-xs" title={text.up} aria-label={text.up} disabled={index === 0} onClick={() => onMove(-1)}><ArrowUp /></Button><Button type="button" variant="ghost" size="icon-xs" title={text.down} aria-label={text.down} disabled={index === count - 1} onClick={() => onMove(1)}><ArrowDown /></Button><Button type="button" variant="ghost" size="icon-xs" title={text.remove} aria-label={text.remove} onClick={onRemove}><Trash2 /></Button></div>
  const linkFields = (link: SiteNavigationLink, update: (value: SiteNavigationLink) => void) => <div className="grid min-w-0 flex-1 gap-2 sm:grid-cols-3"><input className={fieldClass} aria-label={text.label} placeholder={text.label} maxLength={64} value={link.label} onChange={(event) => update({ ...link, label: event.target.value })} /><input className={fieldClass} aria-label={text.english} placeholder={text.english} maxLength={64} value={link.label_en ?? ''} onChange={(event) => update({ ...link, label_en: event.target.value || null })} /><input className={fieldClass} aria-label={text.url} placeholder={text.url} maxLength={2048} value={link.url} onChange={(event) => update({ ...link, url: event.target.value })} /></div>

  return <section className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1/55 shadow-[var(--shadow-subtle)]">
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-4"><div><h3 className="text-sm font-semibold">{text.title}</h3><p className="mt-1 text-xs text-muted-foreground">{text.description}</p></div><Button type="button" size="sm" disabled={mutation.isPending || JSON.stringify(draft) === JSON.stringify(settings.navigation)} onClick={() => void save()}>{mutation.isPending ? text.saving : text.save}</Button></div>
    <div className="grid gap-7 p-4">
      <div><div className="mb-3 flex items-center justify-between"><h4 className="text-sm font-medium">{text.header}</h4><Button type="button" size="xs" variant="secondary" disabled={draft.header_links.length >= 8} onClick={() => setDraft((current) => ({ ...current, header_links: [...current.header_links, emptyLink()] }))}><Plus />{text.addLink}</Button></div><div className="grid gap-2">{draft.header_links.map((link, index) => <div key={index} className="flex min-w-0 items-start gap-2 rounded-md border border-[var(--hairline)] p-2">{linkFields(link, (value) => updateHeader(index, value))}{actions(index, draft.header_links.length, (offset) => setDraft((current) => ({ ...current, header_links: move(current.header_links, index, offset) })), () => setDraft((current) => ({ ...current, header_links: current.header_links.filter((_, position) => position !== index) })))}</div>)}{draft.header_links.length === 0 && <p className="text-xs text-muted-foreground">{text.empty}</p>}</div></div>
      <div><div className="mb-3 flex items-center justify-between"><h4 className="text-sm font-medium">{text.footer}</h4><Button type="button" size="xs" variant="secondary" disabled={draft.footer_groups.length >= 4} onClick={() => setDraft((current) => ({ ...current, footer_groups: [...current.footer_groups, { title: '', title_en: null, links: [emptyLink()] }] }))}><Plus />{text.addGroup}</Button></div><div className="grid gap-3">{draft.footer_groups.map((group, groupIndex) => <div key={groupIndex} className="min-w-0 rounded-md border border-[var(--hairline)] p-3"><div className="flex items-start gap-2"><div className="grid min-w-0 flex-1 gap-2 sm:grid-cols-2"><input className={fieldClass} aria-label={text.groupTitle} placeholder={text.groupTitle} maxLength={64} value={group.title} onChange={(event) => updateGroup(groupIndex, { ...group, title: event.target.value })} /><input className={fieldClass} aria-label={text.groupEnglish} placeholder={text.groupEnglish} maxLength={64} value={group.title_en ?? ''} onChange={(event) => updateGroup(groupIndex, { ...group, title_en: event.target.value || null })} /></div>{actions(groupIndex, draft.footer_groups.length, (offset) => setDraft((current) => ({ ...current, footer_groups: move(current.footer_groups, groupIndex, offset) })), () => setDraft((current) => ({ ...current, footer_groups: current.footer_groups.filter((_, position) => position !== groupIndex) })))}</div><div className="mt-3 grid gap-2">{group.links.map((link, linkIndex) => <div key={linkIndex} className="flex min-w-0 items-start gap-2">{linkFields(link, (value) => updateGroup(groupIndex, { ...group, links: group.links.map((item, position) => position === linkIndex ? value : item) }))}{actions(linkIndex, group.links.length, (offset) => updateGroup(groupIndex, { ...group, links: move(group.links, linkIndex, offset) }), () => updateGroup(groupIndex, { ...group, links: group.links.filter((_, position) => position !== linkIndex) }))}</div>)}<Button type="button" size="xs" variant="ghost" className="justify-self-start" disabled={group.links.length >= 6} onClick={() => updateGroup(groupIndex, { ...group, links: [...group.links, emptyLink()] })}><Plus />{text.addLink}</Button></div></div>)}{draft.footer_groups.length === 0 && <p className="text-xs text-muted-foreground">{text.empty}</p>}</div></div>
      <div><h4 className="mb-3 text-sm font-medium">{text.sidebar}</h4><SidebarNavigationEditor links={draft.sidebar_links} onChange={(sidebar_links) => setDraft((current) => ({ ...current, sidebar_links }))} icons={iconOptions} en={en} /></div>
      {(localError || mutation.isError) && <p role="alert" className="text-sm text-destructive">{localError ? text.invalid : siteSettingsErrorCode(mutation.error) === 'site_settings_conflict' ? text.conflict : text.failed}</p>}
    </div>
  </section>
}
