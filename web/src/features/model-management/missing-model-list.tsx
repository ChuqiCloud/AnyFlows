import { Boxes, RefreshCw, ServerOff } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminMissingModel } from '@/lib/api/generated/types.gen'

type MissingModelListProps = {
  error: boolean
  fetchMore: () => void
  loading: boolean
  loadingMore: boolean
  models: AdminMissingModel[]
  moreError: boolean
  refresh: () => void
  selected: Set<string>
  selectionFull: boolean
  showMore: boolean
  onToggle: (model: string, checked: boolean) => void
}

/** 展示可快速导入的缺失模型，并保留来源渠道证据。 */
export function MissingModelList(props: MissingModelListProps) {
  const { t } = useTranslation()
  if (props.loading) {
    return <div className="grid gap-2" aria-label={t('modelManagement.missing.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-17 rounded-lg" />)}</div>
  }
  if (props.error) {
    return (
      <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
        <div className="flex items-start gap-2.5">
          <ServerOff className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
          <div>
            <h3 className="text-sm font-semibold text-destructive">{t('modelManagement.missing.errorTitle')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.missing.errorBody')}</p>
          </div>
        </div>
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={props.refresh}>
          <RefreshCw aria-hidden="true" />{t('modelManagement.actions.retry')}
        </Button>
      </div>
    )
  }
  if (props.models.length === 0) {
    return (
      <div className="grid min-h-52 place-items-center border-y border-[var(--hairline)] py-8 text-center">
        <div className="max-w-sm">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-success/10 text-success"><Boxes className="size-4" aria-hidden="true" /></div>
          <h3 className="mt-3 text-sm font-semibold">{t('modelManagement.missing.emptyTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.missing.emptyBody')}</p>
          {props.showMore ? (
            <Button type="button" size="sm" variant="secondary" className="mt-3" disabled={props.loadingMore} onClick={props.fetchMore}>
              {t(props.loadingMore ? 'modelManagement.loadingMore' : 'modelManagement.actions.loadMore')}
            </Button>
          ) : null}
        </div>
      </div>
    )
  }

  return (
    <div className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1">
      <div className="divide-y divide-[var(--hairline)]">
        {props.models.map((model) => {
          const checked = props.selected.has(model.model)
          const visibleChannels = model.channels.slice(0, 4)
          const remainingChannels = Math.max(0, model.channel_count - visibleChannels.length)
          return (
            <label key={model.model} className="grid cursor-pointer gap-3 px-3 py-3 transition-colors hover:bg-surface-2/55 sm:grid-cols-[auto_minmax(0,1fr)_minmax(13rem,0.8fr)] sm:items-center">
              <Checkbox
                checked={checked}
                disabled={!checked && props.selectionFull}
                aria-label={t('modelManagement.missing.selectItem', { model: model.model })}
                onCheckedChange={(value) => props.onToggle(model.model, value === true)}
              />
              <div className="min-w-0">
                <div className="truncate font-mono text-xs font-medium" title={model.model}>{model.model}</div>
                <div className="mt-1 text-[0.6875rem] text-muted-foreground">
                  {t('modelManagement.missing.channelCount', { count: model.channel_count })}
                </div>
              </div>
              <div className="flex flex-wrap gap-1 sm:justify-end">
                {visibleChannels.map((channel) => <Badge key={channel.channel_id}>{channel.channel_name}</Badge>)}
                {remainingChannels > 0 ? <Badge>+{remainingChannels}</Badge> : null}
              </div>
            </label>
          )
        })}
      </div>
      {props.moreError ? (
        <div role="alert" className="flex items-center justify-between gap-2 border-t border-destructive/20 bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('modelManagement.missing.moreError')}</span>
          <Button type="button" size="xs" variant="ghost" onClick={props.fetchMore}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {props.showMore && !props.moreError ? (
        <div className="flex justify-center border-t border-[var(--hairline)] p-2">
          <Button type="button" size="sm" variant="ghost" disabled={props.loadingMore} onClick={props.fetchMore}>
            {t(props.loadingMore ? 'modelManagement.loadingMore' : 'modelManagement.actions.loadMore')}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
