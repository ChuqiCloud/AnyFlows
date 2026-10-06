import { CheckCircle2, PencilLine } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import type {
  AdminModelSyncPreviewItem,
  AdminModelSyncRelation,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type ModelSyncPreviewListProps = {
  complete: ReadonlySet<number>
  items: AdminModelSyncPreviewItem[]
  onEdit: (item: AdminModelSyncPreviewItem) => void
  onToggle: (item: AdminModelSyncPreviewItem, selected: boolean) => void
  selected: ReadonlySet<number>
}

const relationOrder: AdminModelSyncRelation[] = [
  'missing_metadata',
  'discovered_unconfigured',
  'existing',
  'not_reported',
]

const relationStyles = {
  missing_metadata: 'border-transparent bg-warning/10 text-warning',
  discovered_unconfigured: 'border-transparent bg-info/10 text-info',
  existing: 'border-transparent bg-success/10 text-success',
  not_reported: 'bg-surface-2 text-muted-foreground',
} as const

export function ModelSyncPreviewList(props: ModelSyncPreviewListProps) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-3">
      {relationOrder.map((relation) => {
        const items = props.items.filter((item) => item.relation === relation)
        if (items.length === 0) return null
        return (
          <section key={relation} className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1" aria-labelledby={`model-sync-relation-${relation}`}>
            <header className="flex items-center justify-between gap-3 bg-surface-2/55 px-3 py-2">
              <div className="flex items-center gap-2">
                <h3 id={`model-sync-relation-${relation}`} className="text-xs font-semibold">{t(`modelManagement.sync.relations.${relation}.title`)}</h3>
                <Badge className={cn('border-transparent', relationStyles[relation])}>{items.length}</Badge>
              </div>
              <p className="hidden text-[0.6875rem] text-muted-foreground sm:block">{t(`modelManagement.sync.relations.${relation}.description`)}</p>
            </header>
            <div className="divide-y divide-[var(--hairline)]">
              {items.map((item) => (
                <PreviewItem
                  key={item.item_id}
                  complete={props.complete.has(item.item_id)}
                  item={item}
                  selected={props.selected.has(item.item_id)}
                  onEdit={() => props.onEdit(item)}
                  onToggle={(selected) => props.onToggle(item, selected)}
                />
              ))}
            </div>
          </section>
        )
      })}
    </div>
  )
}

function PreviewItem({ complete, item, selected, onEdit, onToggle }: {
  complete: boolean
  item: AdminModelSyncPreviewItem
  selected: boolean
  onEdit: () => void
  onToggle: (selected: boolean) => void
}) {
  const { t } = useTranslation()
  const applicable = (item.relation === 'missing_metadata' || item.relation === 'discovered_unconfigured')
    && item.applied_model_id === null
  const evidenceCount = [
    item.display_name_hint,
    item.description_hint,
    item.context_window_hint,
    item.input_token_limit_hint,
    item.output_token_limit_hint,
  ].filter((value) => value !== null).length + item.supported_methods.length

  return (
    <article className="grid gap-2 px-3 py-2.5 sm:grid-cols-[auto_minmax(0,1fr)_minmax(10rem,0.55fr)_auto] sm:items-center">
      <Checkbox
        checked={selected}
        disabled={!applicable}
        aria-label={t('modelManagement.sync.actions.selectItem', { model: item.canonical_model })}
        onCheckedChange={(checked) => onToggle(checked === true)}
      />
      <div className="min-w-0">
        <div className="truncate font-mono text-xs font-medium" title={item.canonical_model}>{item.canonical_model}</div>
        <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
          {item.upstream_model ?? t('modelManagement.sync.preview.notReported')}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge className={cn('border-transparent', relationStyles[item.relation])}>{t(`modelManagement.sync.relations.${item.relation}.badge`)}</Badge>
        {evidenceCount > 0 ? <Badge>{t('modelManagement.sync.preview.evidenceCount', { count: evidenceCount })}</Badge> : null}
        {item.applied_model_id !== null ? <Badge className="border-transparent bg-success/10 text-success"><CheckCircle2 aria-hidden="true" />{t('modelManagement.sync.preview.applied')}</Badge> : null}
        {selected ? <Badge className={complete ? 'border-transparent bg-success/10 text-success' : 'border-transparent bg-warning/10 text-warning'}>{t(complete ? 'modelManagement.sync.preview.ready' : 'modelManagement.sync.preview.incomplete')}</Badge> : null}
      </div>
      <Button type="button" size="sm" variant="ghost" disabled={!applicable} onClick={onEdit}>
        <PencilLine aria-hidden="true" />{t('modelManagement.sync.actions.editItem')}
      </Button>
    </article>
  )
}
