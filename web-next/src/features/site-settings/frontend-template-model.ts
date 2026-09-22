export type TemplateFilter = 'all' | 'builtin' | 'external'

type TemplateItem = {
  id: string
  name: string
  builtin: boolean
  author?: string | null
  description?: string | null
}

/** 在小型元数据目录中筛选分页；图片和模板资源不参与列表计算。 */
export function paginateTemplates<T extends TemplateItem>(templates: readonly T[], filter: TemplateFilter, search: string, requestedPage: number, requestedSize: number) {
  const query = search.trim().toLocaleLowerCase()
  const filtered = templates.filter((template) =>
    (filter === 'all' || (filter === 'builtin' ? template.builtin : !template.builtin))
    && [template.name, template.id, template.author, template.description].some((value) => value?.toLocaleLowerCase().includes(query)),
  )
  const pageSize = Number.isFinite(requestedSize) ? Math.min(24, Math.max(1, Math.floor(requestedSize))) : 8
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize))
  const page = Number.isFinite(requestedPage) ? Math.min(pages, Math.max(1, Math.floor(requestedPage))) : 1
  return { items: filtered.slice((page - 1) * pageSize, page * pageSize), total: filtered.length, page, pages, pageSize }
}
