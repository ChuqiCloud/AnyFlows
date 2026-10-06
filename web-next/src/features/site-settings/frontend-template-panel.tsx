import { Button, Card, CardBody, Chip, Input, Modal, ModalBody, ModalContent, ModalHeader, Select, SelectItem } from '@heroui/react'
import { Check, Eye, LayoutGrid, RefreshCw, Search } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { useActivateFrontendTemplate, useFrontendTemplates, useScanFrontendTemplates, type FrontendTemplateSummary } from './frontend-template-api'
import { paginateTemplates, type TemplateFilter } from './frontend-template-model'
import { FrontendTemplatePreview } from './frontend-template-preview'

const buttonClass = 'gap-1.5 text-xs font-medium'

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
    <section className="rounded-2xl border border-divider bg-content1 p-5 shadow-sm sm:p-6" aria-labelledby="frontend-template-heading">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-2xl">
          <h3 id="frontend-template-heading" className="flex items-center gap-2 text-base font-semibold"><LayoutGrid className="size-4 text-brand" aria-hidden="true" />{t('siteSettings.templates.title')}</h3>
          <p className="mt-1.5 text-xs leading-5 text-muted-foreground">{t('siteSettings.templates.description')}</p>
          {active ? <p className="mt-2 inline-flex items-center gap-1 rounded-full bg-success/10 px-2.5 py-1 text-xs text-success"><Check className="size-3" aria-hidden="true" />{t('siteSettings.templates.active', { id: active.name })}</p> : null}
        </div>
        <div className="flex gap-2">
          <Button size="sm" variant="bordered" type="button" className={buttonClass} isDisabled={busy || catalogQuery.isFetching} onClick={() => void catalogQuery.refetch()}><RefreshCw className={catalogQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('siteSettings.templates.refresh')}</Button>
          <Button size="sm" variant="bordered" type="button" className={buttonClass} isDisabled={busy || catalogQuery.isPending} onClick={() => scanMutation.mutate()}>{t('siteSettings.templates.scan')}</Button>
        </div>
      </header>
      <div className="my-5 flex flex-wrap items-center justify-between gap-3">
        <div className="flex gap-1 rounded-lg bg-surface-2/70 p-1" role="group" aria-label={t('siteSettings.templates.filterLabel')}>
          {(['all', 'builtin', 'external'] as const).map((kind) => (
            <Button size="sm" variant={filter === kind ? "flat" : "light"} color={filter === kind ? "primary" : "default"} key={kind} type="button" aria-pressed={filter === kind} className={'rounded-md px-3 py-1.5 text-xs font-medium ' + (filter === kind ? 'bg-surface-1 text-foreground shadow-sm' : 'text-muted-foreground')} onClick={() => { setFilter(kind); setPage(1) }}>
              {t(`siteSettings.templates.filters.${kind}`)} <span className="ml-1 tabular-nums opacity-60">{templates.filter((item) => kind === 'all' || (kind === 'builtin' ? item.builtin : !item.builtin)).length}</span>
            </Button>
          ))}
        </div>
        <Input size="sm" variant="bordered" className="w-full sm:max-w-xs" aria-label={t('siteSettings.templates.search')} startContent={<Search className="size-4 text-default-400" aria-hidden="true" />} value={search} onValueChange={(value) => { setSearch(value); setPage(1) }} placeholder={t('siteSettings.templates.search')} />
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
          <label className="flex items-center gap-1.5">{t('siteSettings.templates.pageSize')}<Select size="sm" aria-label={t('siteSettings.templates.pageSize')} className="w-20" disallowEmptySelection selectedKeys={[String(pageSize)]} onSelectionChange={(keys) => { setPageSize(Number(Array.from(keys)[0] ?? 8)); setPage(1) }}>{[8, 12, 24].map((size) => <SelectItem key={String(size)} textValue={String(size)}>{String(size)}</SelectItem>)}</Select></label>
          <Button size="sm" variant="bordered" type="button" className={buttonClass} isDisabled={pagination.page === 1} onClick={() => setPage(pagination.page - 1)}>{t('siteSettings.templates.previousPage')}</Button>
          <Button size="sm" variant="bordered" type="button" className={buttonClass} isDisabled={pagination.page === pagination.pages} onClick={() => setPage(pagination.page + 1)}>{t('siteSettings.templates.nextPage')}</Button>
        </div>
      </footer> : null}
      <p className="mt-4 text-xs leading-5 text-muted-foreground">{t('siteSettings.templates.installHint')}</p>
      <TemplateDialog open={Boolean(preview)} title={t('siteSettings.templates.preview')} onClose={() => setPreview(undefined)}>
        {preview ? <><h4 className="mb-3 text-sm font-semibold">{preview.name}</h4><div className="aspect-[16/10] overflow-hidden rounded-xl border border-[var(--hairline)]"><FrontendTemplatePreview template={preview} /></div><p className="mt-3 text-xs leading-5 text-muted-foreground">{t(preview.builtin ? 'siteSettings.templates.builtinPreviewHint' : 'siteSettings.templates.previewHint')}</p></> : null}
      </TemplateDialog>
      <TemplateDialog open={Boolean(pending)} title={t('siteSettings.templates.confirmTitle')} onClose={() => { if (!activateMutation.isPending) setPending(undefined) }} locked={activateMutation.isPending}>
        <p className="text-sm leading-6">{t('siteSettings.templates.confirmBody', { name: pending?.name })}</p>
        {activateMutation.isError ? <p role="alert" className="mt-3 text-xs text-destructive">{t('siteSettings.templates.error')}</p> : null}
        <div className="mt-5 flex justify-end gap-2"><Button size="sm" variant="bordered" type="button" className={buttonClass} isDisabled={activateMutation.isPending} onClick={() => setPending(undefined)}>{t('siteSettings.templates.cancel')}</Button><Button size="sm" variant="bordered" type="button" className={buttonClass + ' text-brand'} isDisabled={activateMutation.isPending} onClick={apply}>{t(activateMutation.isPending ? 'siteSettings.templates.applying' : 'siteSettings.templates.confirmApply')}</Button></div>
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
    <Card as="article" shadow="sm" className={'flex min-w-0 flex-col overflow-hidden rounded-2xl border bg-content1 transition-shadow hover:shadow-lg ' + (active ? 'border-brand ring-1 ring-brand/20' : 'border-[var(--hairline)]')}>
      <Button size="sm" variant="bordered" type="button" className="relative block h-auto min-w-0 rounded-none p-0 aspect-[16/10] w-full overflow-hidden border-b border-[var(--hairline)] bg-surface-2/40" isDisabled={!template.preview_url} onClick={onPreview} aria-label={t('siteSettings.templates.previewAlt', { name: template.name || template.id })}>
        <FrontendTemplatePreview template={template} />
        <Chip size="sm" variant="flat" className="absolute left-3 top-3 bg-content1/95 text-[0.625rem]">{t(template.builtin ? 'siteSettings.templates.filters.builtin' : 'siteSettings.templates.filters.external')}</Chip>
        {active ? <Chip size="sm" color="success" variant="solid" className="absolute right-3 top-3 text-[0.625rem]">{t('siteSettings.templates.enabled')}</Chip> : null}
      </Button>
      <CardBody className="flex flex-1 flex-col gap-3 p-4">
        <div className="flex items-baseline justify-between gap-2"><h4 className="truncate text-sm font-semibold" title={template.name || template.id}>{template.name || template.id}</h4><span className="shrink-0 text-[0.625rem] text-muted-foreground">{template.version ? `v${template.version}` : t('siteSettings.templates.invalid')}</span></div>
        <p className="min-h-10 text-xs leading-5 text-muted-foreground">{description}</p>
        <p className="truncate text-[0.625rem] text-muted-foreground">{template.author || template.id}{template.api_contract ? ` · API ${template.api_contract}` : ''}</p>
        {!template.valid ? <p className="rounded-md bg-destructive/10 p-2 text-xs text-destructive">{template.error || t('siteSettings.templates.invalidHint')}</p> : null}
        <div className="mt-auto flex gap-2 border-t border-[var(--hairline)] pt-3">
          <Button size="sm" variant="bordered" type="button" className={buttonClass + ' flex-1'} isDisabled={!template.preview_url} onClick={onPreview}><Eye className="size-3.5" aria-hidden="true" />{t('siteSettings.templates.preview')}</Button>
          <Button size="sm" variant="bordered" type="button" className={buttonClass + ' flex-1 text-brand'} isDisabled={busy || !template.valid || active} onClick={onActivate}>{t(active ? 'siteSettings.templates.enabled' : 'siteSettings.templates.apply')}</Button>
        </div>
      </CardBody>
    </Card>
  )
}

function TemplateDialog({ open, title, onClose, locked = false, children }: {
  open: boolean
  title: string
  onClose: () => void
  locked?: boolean
  children: ReactNode
}) {
  return (
    <Modal isOpen={open} onOpenChange={(value) => { if (!value && !locked) onClose() }} isDismissable={!locked} isKeyboardDismissDisabled={locked} hideCloseButton={locked} size="3xl" scrollBehavior="inside" backdrop="blur" classNames={{ base: 'rounded-2xl border border-divider bg-content1', closeButton: 'top-4 right-4' }}>
      <ModalContent><ModalHeader>{title}</ModalHeader><ModalBody className="pb-6">{children}</ModalBody></ModalContent>
    </Modal>
  )
}
