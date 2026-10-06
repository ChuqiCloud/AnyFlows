import { providerDisplayName } from '@/components/brand/provider-catalog'
import { Button, Chip } from '@heroui/react'
import { Boxes, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { findProviderLogo } from '@/components/brand/model-logos'
import type { AdminModel, AdminModelModality } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { SiteTooltip } from '@/shared/components/site-tooltip'

type ModelManagementTableProps = {
  models: AdminModel[]
  onDelete: (model: AdminModel) => void
  onEdit: (model: AdminModel) => void
}

const lifecycleStyles = {
  draft: 'bg-surface-2 text-muted-foreground',
  active: 'bg-success/10 text-success',
  deprecated: 'bg-warning/10 text-warning',
  retired: 'bg-destructive/10 text-destructive',
} as const

const visibilityStyles = {
  public: 'bg-info/10 text-info',
  authenticated: 'bg-primary/10 text-primary',
  hidden: 'bg-surface-2 text-muted-foreground',
} as const

export function ModelManagementTable({ models, onDelete, onEdit }: ModelManagementTableProps) {
  const { t } = useTranslation()
  if (models.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Boxes className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('modelManagement.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[30%] px-3 py-2 font-medium">{t('modelManagement.columns.model')}</th>
              <th className="w-[20%] px-3 py-2 font-medium">{t('modelManagement.columns.operation')}</th>
              <th className="w-[34%] px-3 py-2 font-medium">{t('modelManagement.columns.capabilities')}</th>
              <th className="w-[16%] px-3 py-2"><span className="sr-only">{t('modelManagement.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {models.map((model) => (
              <tr key={model.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-3"><ModelIdentity model={model} /></td>
                <td className="px-3 py-3"><ModelOperation model={model} /></td>
                <td className="px-3 py-3"><ModelCapabilities model={model} /></td>
                <td className="px-2 py-3"><RowActions model={model} onDelete={onDelete} onEdit={onEdit} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {models.map((model) => (
          <article key={model.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <ModelIdentity model={model} />
              <RowActions model={model} onDelete={onDelete} onEdit={onEdit} />
            </div>
            <div className="mt-3 grid gap-3 border-t border-[var(--hairline)] pt-3 sm:grid-cols-2">
              <ModelOperation model={model} />
              <ModelCapabilities model={model} />
            </div>
          </article>
        ))}
      </div>
    </>
  )
}

function ModelIdentity({ model }: { model: AdminModel }) {
  const { t } = useTranslation()
  const logo = findProviderLogo(model.provider)
  const ProviderIcon = logo?.Icon
  return (
    <div className="flex min-w-0 items-start gap-2.5">
      <div className="relative grid size-9 shrink-0 place-items-center overflow-hidden rounded-lg bg-surface-2 text-muted-foreground">
        {ProviderIcon ? <ProviderIcon className="size-4" aria-hidden="true" /> : <Boxes className="size-4" aria-hidden="true" />}
        {model.icon_url ? <img key={model.icon_url} src={model.icon_url} alt="" className="absolute inset-0 size-full object-cover" onError={(event) => { event.currentTarget.hidden = true }} /> : null}
      </div>
      <div className="min-w-0">
        <div className="truncate font-medium">{model.display_name}</div>
        <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{model.model}</div>
        <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.values.providerId', { provider: providerDisplayName(model.provider), id: model.id })}</div>
        {model.tags.length > 0 ? (
          <div className="mt-2 flex flex-wrap gap-1">
            {model.tags.slice(0, 3).map((tag) => <Chip key={tag} size="sm" variant="flat">{tag}</Chip>)}
            {model.tags.length > 3 ? <Chip size="sm" variant="flat">{t('modelManagement.values.moreTags', { count: model.tags.length - 3 })}</Chip> : null}
          </div>
        ) : null}
      </div>
    </div>
  )
}

function ModelOperation({ model }: { model: AdminModel }) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="flex flex-wrap gap-1">
        <Chip className={cn(lifecycleStyles[model.lifecycle])} size="sm" variant="flat">{t(`modelManagement.lifecycle.${model.lifecycle}`)}</Chip>
        <Chip className={cn(visibilityStyles[model.visibility])} size="sm" variant="flat">{t(`modelManagement.visibility.${model.visibility}`)}</Chip>
      </div>
      <div className="mt-2 tabular-nums text-muted-foreground">
        {model.context_window === null
          ? t('modelManagement.values.contextUnknown')
          : t('modelManagement.values.context', { value: new Intl.NumberFormat().format(model.context_window) })}
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.values.updated', { value: formatDate(model.updated_at) })}</div>
    </div>
  )
}

function ModelCapabilities({ model }: { model: AdminModel }) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.values.input')}</span>
        {model.input_modalities.map((modality) => <ModalityBadge key={modality} modality={modality} />)}
      </div>
      <div className="flex flex-wrap items-center gap-1">
        <span className="mr-1 text-[0.6875rem] text-muted-foreground">{t('modelManagement.values.output')}</span>
        {model.output_modalities.map((modality) => <ModalityBadge key={modality} modality={modality} />)}
      </div>
      {(model.supports_reasoning || model.supports_tool_calls) ? (
        <div className="flex flex-wrap gap-1">
          {model.supports_reasoning ? <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('modelManagement.capabilities.reasoning')}</Chip> : null}
          {model.supports_tool_calls ? <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('modelManagement.capabilities.toolCalls')}</Chip> : null}
        </div>
      ) : null}
    </div>
  )
}

function ModalityBadge({ modality }: { modality: AdminModelModality }) {
  const { t } = useTranslation()
  return <Chip size="sm" variant="flat">{t(`modelManagement.modalities.${modality}`)}</Chip>
}

function RowActions({ model, onDelete, onEdit }: {
  model: AdminModel
  onDelete: (model: AdminModel) => void
  onEdit: (model: AdminModel) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <SiteTooltip content={t('modelManagement.actions.edit')}>
        <Button isIconOnly type="button" size="sm" variant="light" className="size-10 md:size-8" aria-label={t('modelManagement.actions.edit')} onClick={() => onEdit(model)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
      <SiteTooltip content={t('modelManagement.actions.delete')}>
        <Button isIconOnly type="button" size="sm" variant="light" className="size-10 text-muted-foreground hover:text-destructive md:size-8" aria-label={t('modelManagement.actions.delete')} onClick={() => onDelete(model)}><Trash2 className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
    </div>
  )
}

function formatDate(timestamp: number) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(new Date(timestamp * 1000))
}
