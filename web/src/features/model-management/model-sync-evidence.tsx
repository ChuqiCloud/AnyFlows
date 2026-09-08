import { ArrowDownToLine, Database } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'

type ModelSyncEvidenceProps = {
  item: AdminModelSyncPreviewItem
  onUseContext: () => void
  onUseDescription: () => void
  onUseDisplayName: () => void
}

/** 只展示上游明确返回的证据，并由管理员逐项决定是否采纳。 */
export function ModelSyncEvidence(props: ModelSyncEvidenceProps) {
  const { t } = useTranslation()
  const { item } = props
  const limits = [
    ['context', item.context_window_hint],
    ['input', item.input_token_limit_hint],
    ['output', item.output_token_limit_hint],
  ] as const

  return (
    <section className="rounded-xl bg-surface-2/45 p-3" aria-labelledby="model-sync-evidence-title">
      <div className="flex items-center gap-2">
        <Database className="size-3.5 text-info" aria-hidden="true" />
        <h3 id="model-sync-evidence-title" className="text-xs font-semibold">
          {t('modelManagement.sync.evidence.title')}
        </h3>
      </div>
      <p className="mt-1 text-[0.6875rem] leading-5 text-muted-foreground">
        {t('modelManagement.sync.evidence.disclaimer')}
      </p>

      <dl className="mt-3 grid gap-2 text-xs">
        <EvidenceRow label={t('modelManagement.sync.evidence.upstreamModel')} value={item.upstream_model} />
        <EvidenceRow
          label={t('modelManagement.fields.displayName')}
          value={item.display_name_hint}
          onUse={item.display_name_hint ? props.onUseDisplayName : undefined}
        />
        <EvidenceRow
          label={t('modelManagement.fields.description')}
          value={item.description_hint}
          onUse={item.description_hint ? props.onUseDescription : undefined}
        />
      </dl>

      <div className="mt-3 flex flex-wrap gap-1.5">
        {limits.map(([key, value]) => value === null ? null : (
          <Badge key={key} className="h-6 bg-surface-1 font-mono text-muted-foreground">
            {t(`modelManagement.sync.evidence.${key}Limit`, { value: new Intl.NumberFormat().format(value) })}
          </Badge>
        ))}
        {item.context_window_hint !== null ? (
          <Button type="button" size="xs" variant="ghost" onClick={props.onUseContext}>
            <ArrowDownToLine aria-hidden="true" />
            {t('modelManagement.sync.evidence.useContext')}
          </Button>
        ) : null}
      </div>

      {item.supported_methods.length > 0 ? (
        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          <span className="text-[0.6875rem] text-muted-foreground">
            {t('modelManagement.sync.evidence.methods')}
          </span>
          {item.supported_methods.map((method) => <Badge key={method}>{method}</Badge>)}
        </div>
      ) : null}
    </section>
  )
}

function EvidenceRow({ label, value, onUse }: {
  label: string
  value: string | null
  onUse?: () => void
}) {
  const { t } = useTranslation()
  return (
    <div className="grid grid-cols-[7rem_minmax(0,1fr)_auto] items-start gap-2">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0 break-words">{value ?? t('modelManagement.sync.evidence.none')}</dd>
      {onUse ? (
        <Button type="button" size="icon-xs" variant="ghost" aria-label={t('modelManagement.sync.evidence.use', { label })} onClick={onUse}>
          <ArrowDownToLine aria-hidden="true" />
        </Button>
      ) : <span className="size-6" aria-hidden="true" />}
    </div>
  )
}
