import { Check, Eye, LayoutGrid, RefreshCw, Search, X } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { useActivateFrontendTemplate, useFrontendTemplates, useScanFrontendTemplates, type FrontendTemplateSummary } from './frontend-template-api'
import { paginateTemplates, type TemplateFilter } from './frontend-template-model'
import { FrontendTemplatePreview } from './frontend-template-preview'

const buttonClass = 'inline-flex min-h-9 items-center justify-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-surface-1 px-3 text-xs font-medium transition-colors hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand disabled:cursor-not-allowed disabled:opacity-45'

/** 预览只读取图片；只有确认启用才修改全局前端选择。 */
export function FrontendTemplatePanel() {
  const { t } = useTranslation()
  const catalogQuery = useFrontendTemplates()
  const scanMutation = useScanFrontendTemplates()
  const activateMutation = useActivateFrontendTemplate()
  const [filter, setFilter] = useState<TemplateFilter>('all')
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(8)
  const [search, setSearch] = useState('')
  const [preview, setPreview] = useState<FrontendTemplateSummary>()
  const [pending, setPending] = useState<FrontendTemplateSummary>()
  const activeId = catalogQuery.data?.active_id ?? 'embedded'
  const busy = scanMutation.isPending || activateMutation.isPending
  const templates = catalogQuery.data?.templates ?? []
  const active = templates.find((template) => template.id === activeId)
  const pagination = paginateTemplates(templates, filter, search, page, pageSize)
  const visible = pagination.items
  const apply = () => {
    if (!pending || busy) return
    activateMutation.mutate(pending.id === 'embedded' ? null : pending.id, {
      onSuccess: () => window.location.reload(),
    })
  }

  return (
    <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/55 p-4 sm:p-5" aria-labelledby="frontend-template-heading">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-2xl">
          <h3 id="frontend-template-heading" className="flex items-center gap-2 text-base font-semibold"><LayoutGrid className="size-4 text-brand" aria-hidden="true" />{t('siteSettings.templates.title')}</h3>
          <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{t('siteSettings.templates.description')}</p>
          {active ? <p className="mt-2 inline-flex items-center gap-1 rounded-full bg-success/10 px-2.5 py-1 text-xs text-success"><Check className="size-3" aria-hidden="true" />{t('siteSettings.templates.active', { id: active.name })}</p> : null}
        </div>
        <div className="flex gap-2">
          <button type="button" className={buttonClass} disabled={busy || catalogQuery.isFetching} onClick={() => void catalogQuery.refetch()}><RefreshCw className={catalogQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('siteSettings.templates.refresh')}</button>
          <button type="button" className={buttonClass} disabled={busy || catalogQuery.isPending} onClick={() => scanMutation.mutate()}>{t('siteSettings.templates.scan')}</button>
        </div>
      </header>
      <div className="my-5 flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-1 rounded-lg bg-surface-2/70 p-1" role="group" aria-label={t('siteSettings.templates.filterLabel')}>
          {(['all', 'builtin', 'external'] as const).map((kind) => (
            <button key={kind} type="button" aria-pressed={filter === kind} className={'rounded-md px-3 py-1.5 text-xs font-medium ' + (filter === kind ? 'bg-surface-1 text-foreground shadow-sm' : 'text-muted-foreground')} onClick={() => { setFilter(kind); setPage(1) }}>
              {t(`siteSettings.templates.filters.${kind}`)} <span className="ml-1 tabular-nums opacity-60">{templates.filter((item) => kind === 'all' || (kind === 'builtin' ? item.builtin : !item.builtin)).length}</span>
            </button>
          ))}
        </div>
        <label className="flex min-w-48 items-center gap-2 rounded-lg border border-[var(--hairline)] bg-surface-1 px-3 py-2 text-xs">
          <Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" /><span className="sr-only">{t('siteSettings.templates.search')}</span>
          <input className="w-full bg-transparent outline-none" value={search} onChange={(event) => { setSearch(event.target.value); setPage(1) }} placeholder={t('siteSettings.templates.search')} />
        </label>
      </div>
      {catalogQuery.isError || scanMutation.isError ? <p role="alert" className="mb-4 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">{t('siteSettings.templates.error')}</p> : null}
      {catalogQuery.isPending ? (
        <div role="status" aria-label={t('siteSettings.templates.loading')} className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">{[0, 1].map((index) => <div key={index} className="aspect-square animate-pulse rounded-xl bg-surface-2" />)}</div>
      ) : (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {visible.map((template) => <TemplateCard key={template.id} template={template} active={template.id === activeId} busy={busy} onPreview={() => setPreview(template)} onActivate={() => { activateMutation.reset(); setPending(template) }} />)}
        </div>
      )}
      {!catalogQuery.isPending && !catalogQuery.isError && !visible.length ? <p className="rounded-xl border border-dashed border-[var(--hairline)] p-8 text-center text-sm text-muted-foreground">{t('siteSettings.templates.empty')}</p> : null}
      {pagination.total > 0 ? <footer className="mt-4 flex flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] pt-3 text-xs text-muted-foreground">
        <span aria-live="polite">{t('siteSettings.templates.pageSummary', { page: pagination.page, pages: pagination.pages, count: pagination.total })}</span>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-1.5">{t('siteSettings.templates.pageSize')}<select className="rounded-md border border-[var(--hairline)] bg-surface-1 px-2 py-1.5" value={pageSize} onChange={(event) => { setPageSize(Number(event.target.value)); setPage(1) }}>{[8, 12, 24].map((size) => <option key={size} value={size}>{size}</option>)}</select></label>
          <button type="button" className={buttonClass} disabled={pagination.page === 1} onClick={() => setPage(pagination.page - 1)}>{t('siteSettings.templates.previousPage')}</button>
          <button type="button" className={buttonClass} disabled={pagination.page === pagination.pages} onClick={() => setPage(pagination.page + 1)}>{t('siteSettings.templates.nextPage')}</button>
        </div>
      </footer> : null}
      <p className="mt-4 text-xs leading-5 text-muted-foreground">{t('siteSettings.templates.installHint')}</p>
      <TemplateDialog open={Boolean(preview)} title={t('siteSettings.templates.preview')} onClose={() => setPreview(undefined)}>
        {preview ? <><h4 className="mb-3 text-sm font-semibold">{preview.name}</h4><div className="aspect-[16/10] overflow-hidden rounded-xl border border-[var(--hairline)]"><FrontendTemplatePreview template={preview} /></div><p className="mt-3 text-xs leading-5 text-muted-foreground">{t(preview.builtin ? 'siteSettings.templates.builtinPreviewHint' : 'siteSettings.templates.previewHint')}</p></> : null}
      </TemplateDialog>
      <TemplateDialog open={Boolean(pending)} title={t('siteSettings.templates.confirmTitle')} onClose={() => { if (!activateMutation.isPending) setPending(undefined) }} locked={activateMutation.isPending}>
        <p className="text-sm leading-6">{t('siteSettings.templates.confirmBody', { name: pending?.name })}</p>
        {activateMutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('siteSettings.templates.error')}</p> : null}
        <div className="mt-5 flex justify-end gap-2"><button type="button" className={buttonClass} disabled={activateMutation.isPending} onClick={() => setPending(undefined)}>{t('siteSettings.templates.cancel')}</button><button type="button" className={buttonClass + ' text-brand'} disabled={activateMutation.isPending} onClick={apply}>{t(activateMutation.isPending ? 'siteSettings.templates.applying' : 'siteSettings.templates.confirmApply')}</button></div>
      </TemplateDialog>
    </section>
  )
}

function TemplateCard({ template, active, busy, onPreview, onActivate }: {
  template: FrontendTemplateSummary
  active: boolean
  busy: boolean
  onPreview: () => void
  onActivate: () => void
}) {
  const { t } = useTranslation()
  const description = template.description ?? (template.builtin ? t(template.id === 'embedded-next' ? 'siteSettings.templates.nextDescription' : 'siteSettings.templates.classicDescription') : t('siteSettings.templates.externalDescription'))
  return (
    <article className={'flex min-w-0 flex-col overflow-hidden rounded-lg border bg-surface-1 transition-shadow hover:shadow-md ' + (active ? 'border-brand ring-1 ring-brand/20' : 'border-[var(--hairline)]')}>
      <button type="button" className="relative block aspect-[16/10] w-full overflow-hidden border-b border-[var(--hairline)] bg-surface-2/40" disabled={!template.preview_url} onClick={onPreview} aria-label={t('siteSettings.templates.previewAlt', { name: template.name || template.id })}>
        <FrontendTemplatePreview template={template} />
        <span className="absolute left-2 top-2 rounded-full border border-[var(--hairline)] bg-surface-1/95 px-2 py-1 text-[0.625rem] font-medium">{t(template.builtin ? 'siteSettings.templates.filters.builtin' : 'siteSettings.templates.filters.external')}</span>
        {active ? <span className="absolute right-2 top-2 rounded-full bg-success px-2 py-1 text-[0.625rem] font-semibold text-white">{t('siteSettings.templates.enabled')}</span> : null}
      </button>
      <div className="flex flex-1 flex-col gap-2.5 p-3.5">
        <div className="flex items-baseline justify-between gap-2"><h4 className="truncate text-sm font-semibold" title={template.name || template.id}>{template.name || template.id}</h4><span className="shrink-0 text-[0.625rem] text-muted-foreground">{template.version ? `v${template.version}` : t('siteSettings.templates.invalid')}</span></div>
        <p className="min-h-10 text-xs leading-5 text-muted-foreground">{description}</p>
        <p className="truncate text-[0.625rem] text-muted-foreground">{template.author || template.id}{template.api_contract ? ` · API ${template.api_contract}` : ''}</p>
        {!template.valid ? <p className="rounded-md bg-destructive/10 p-2 text-xs text-destructive">{template.error || t('siteSettings.templates.invalidHint')}</p> : null}
        <div className="mt-auto flex gap-2 border-t border-[var(--hairline)] pt-3">
          <button type="button" className={buttonClass + ' flex-1'} disabled={!template.preview_url} onClick={onPreview}><Eye className="size-3.5" aria-hidden="true" />{t('siteSettings.templates.preview')}</button>
          <button type="button" className={buttonClass + ' flex-1 text-brand'} disabled={busy || !template.valid || active} onClick={onActivate}>{t(active ? 'siteSettings.templates.enabled' : 'siteSettings.templates.apply')}</button>
        </div>
      </div>
    </article>
  )
}

function TemplateDialog({ open, title, onClose, locked = false, children }: {
  open: boolean
  title: string
  onClose: () => void
  locked?: boolean
  children: ReactNode
}) {
  const ref = useRef<HTMLDialogElement>(null)
  const { t } = useTranslation()
  useEffect(() => {
    const dialog = ref.current
    if (!dialog) return
    if (open && !dialog.open) dialog.showModal()
    if (!open && dialog.open) dialog.close()
  }, [open])
  return (
    <dialog ref={ref} aria-label={title} onCancel={(event) => { event.preventDefault(); if (!locked) onClose() }} onClick={(event) => { if (event.target === event.currentTarget && !locked) onClose() }} className="fixed inset-0 m-auto max-h-[90vh] w-[min(52rem,calc(100vw-2rem))] overflow-auto rounded-xl border border-[var(--hairline)] bg-surface-1 p-0 text-foreground shadow-xl backdrop:bg-black/60">
      <div className="p-5"><div className="mb-4 flex items-center justify-between gap-4"><h3 className="font-semibold">{title}</h3><button type="button" className={buttonClass + ' px-2'} disabled={locked} onClick={onClose} aria-label={t('siteSettings.templates.close')}><X className="size-4" aria-hidden="true" /></button></div>{children}</div>
    </dialog>
  )
}
