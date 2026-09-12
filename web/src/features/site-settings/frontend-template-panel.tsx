import { RefreshCw, RotateCcw } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import {
  useActivateFrontendTemplate,
  useFrontendTemplates,
  useScanFrontendTemplates,
} from './frontend-template-api'

/** 管理运行目录模板的扫描、选择和内嵌回退。 */
export function FrontendTemplatePanel() {
  const { t } = useTranslation()
  const catalogQuery = useFrontendTemplates()
  const scanMutation = useScanFrontendTemplates()
  const activateMutation = useActivateFrontendTemplate()
  const [selected, setSelected] = useState('embedded')

  useEffect(() => {
    if (catalogQuery.data) {
      setSelected(catalogQuery.data.active_id ?? 'embedded')
    }
  }, [catalogQuery.data])

  const catalog = catalogQuery.data
  const activeId = catalog?.active_id ?? 'embedded'
  const busy = scanMutation.isPending || activateMutation.isPending
  const apply = () => {
    const next = selected === 'embedded' ? null : selected
    void activateMutation.mutateAsync(next)
  }

  return (
    <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/55 p-4 shadow-[var(--shadow-subtle)]">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold">{t('siteSettings.templates.title')}</h3>
            <Badge className="border-info/25 bg-info/10 text-info">
              {activeId === 'embedded'
                ? t('siteSettings.templates.embedded')
                : t('siteSettings.templates.active', { id: activeId })}
            </Badge>
          </div>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('siteSettings.templates.description')}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={busy || catalogQuery.isFetching}
            onClick={() => void catalogQuery.refetch()}
          >
            <RefreshCw className={catalogQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('siteSettings.templates.refresh')}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={busy}
            onClick={() => void scanMutation.mutateAsync()}
          >
            <RotateCcw className={scanMutation.isPending ? 'animate-spin' : undefined} aria-hidden="true" />
            {t('siteSettings.templates.scan')}
          </Button>
        </div>
      </div>

      <div className="mt-4 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <label className="grid gap-1.5 text-xs font-medium" htmlFor="frontend-template-select">
          {t('siteSettings.templates.selector')}
          <Select
            id="frontend-template-select"
            value={selected}
            disabled={busy || catalogQuery.isPending}
            onChange={(event) => setSelected(event.target.value)}
          >
            <option value="embedded">{t('siteSettings.templates.embeddedOption')}</option>
            {catalog?.templates.map((template) => (
              <option key={template.id} value={template.id} disabled={!template.valid}>
                {template.valid
                  ? template.name + ' (' + template.version + ')'
                  : template.id + ' — ' + t('siteSettings.templates.invalid')}
              </option>
            ))}
          </Select>
        </label>
        <Button
          type="button"
          size="sm"
          disabled={busy || selected === activeId || !catalog}
          onClick={apply}
        >
          {activateMutation.isPending
            ? t('siteSettings.templates.applying')
            : t('siteSettings.templates.apply')}
        </Button>
      </div>

      {catalogQuery.isError || scanMutation.isError || activateMutation.isError ? (
        <p role="alert" className="mt-3 text-xs text-destructive">
          {t('siteSettings.templates.error')}
        </p>
      ) : null}
      {catalog && catalog.templates.some((template) => !template.valid) ? (
        <p className="mt-3 text-xs leading-5 text-muted-foreground">
          {t('siteSettings.templates.invalidHint')}
        </p>
      ) : null}
    </section>
  )
}
