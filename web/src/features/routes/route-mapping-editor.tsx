import { Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type { RouteFormErrorCode, RouteMappingRow } from './route-form-model'
import { createRouteRowId } from './route-form-model'

type RouteMappingEditorProps = {
  rows: RouteMappingRow[]
  error?: RouteFormErrorCode
  onChange: (rows: RouteMappingRow[]) => void
}

/** 用逐行键值控件编辑模型映射，避免管理员直接维护一段 JSON。 */
export function RouteMappingEditor({ rows, error, onChange }: RouteMappingEditorProps) {
  const { t } = useTranslation()
  const update = (id: string, patch: Partial<RouteMappingRow>) => {
    onChange(rows.map((row) => row.id === id ? { ...row, ...patch } : row))
  }

  return (
    <div className="grid gap-2">
      {rows.length > 0 ? (
        <div className="grid gap-2">
          <div className="hidden grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem] gap-2 px-1 text-[0.6875rem] text-muted-foreground sm:grid">
            <span>{t('routes.form.mappingSource')}</span>
            <span>{t('routes.form.mappingTarget')}</span>
          </div>
          {rows.map((row) => (
            <div key={row.id} className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem]">
              <Input
                value={row.source}
                placeholder={t('routes.form.mappingSourcePlaceholder')}
                aria-label={t('routes.form.mappingSource')}
                onChange={(event) => update(row.id, { source: event.target.value })}
              />
              <Input
                value={row.target}
                className="col-start-1 font-mono text-xs sm:col-start-2 sm:row-start-1"
                placeholder={t('routes.form.mappingTargetPlaceholder')}
                aria-label={t('routes.form.mappingTarget')}
                onChange={(event) => update(row.id, { target: event.target.value })}
              />
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                className="col-start-2 row-start-1 text-muted-foreground hover:text-destructive sm:col-start-3"
                aria-label={t('routes.form.removeMapping')}
                title={t('routes.form.removeMapping')}
                onClick={() => onChange(rows.filter((item) => item.id !== row.id))}
              >
                <Trash2 aria-hidden="true" />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-xs text-muted-foreground">
          {t('routes.form.mappingEmpty')}
        </p>
      )}
      {error ? <p className="text-xs text-destructive">{t(`routes.validation.${error}`)}</p> : null}
      <Button
        type="button"
        size="sm"
        variant="secondary"
        className="w-fit"
        onClick={() => onChange([...rows, { id: createRouteRowId('mapping'), source: '', target: '' }])}
      >
        <Plus aria-hidden="true" />
        {t('routes.form.addMapping')}
      </Button>
    </div>
  )
}
