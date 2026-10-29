import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import type { UserToken } from '@/lib/api/generated/types.gen'
import { ApiKeyField } from './api-key-form-field'
import { formatGroupBaseRatio, type SelectableTokenGroup } from './api-key-group-model'

type ApiKeyGroupSelectorProps = {
  token?: UserToken
  groups?: SelectableTokenGroup[]
  value: string
  loading: boolean
  failed: boolean
  error?: string
  onChange: (value: string) => void
  onRetry: () => void
}

export function ApiKeyGroupSelector(props: ApiKeyGroupSelectorProps) {
  const { t } = useTranslation()
  const defaultGroup = props.groups?.find((group) => group.is_default)
  const groupLabel = (group: SelectableTokenGroup) => t('apiKeys.group.option', {
    name: group.display_name,
    ratio: formatGroupBaseRatio(group.ratio_micros),
  })
  return (
    <ApiKeyField id="api-key-group" label={t('apiKeys.group.label')} error={props.error} hint={t('apiKeys.group.pricingHint')}>
      {props.token ? (
        <>
          <p id="api-key-group" className="rounded-lg border border-[var(--hairline)] px-3 py-2 text-sm">
            {props.token.group_id === null
              ? t('apiKeys.group.following', { name: props.token.group_display_name })
              : props.token.group_display_name}
          </p>
          <p className="text-xs text-muted-foreground">{t('apiKeys.group.immutable')}</p>
        </>
      ) : (
        <>
          <Select id="api-key-group" value={props.value} disabled={props.loading || props.failed || !props.groups} aria-invalid={!!props.error} onChange={(event) => props.onChange(event.target.value)}>
            <option value="">{defaultGroup ? t('apiKeys.group.followOption', { group: groupLabel(defaultGroup) }) : t('apiKeys.group.followDefault')}</option>
            {props.groups?.map((group) => <option key={group.id} value={group.id}>{groupLabel(group)}</option>)}
          </Select>
          {props.loading ? <p role="status" className="text-xs text-muted-foreground">{t('apiKeys.group.loading')}</p> : null}
          {props.failed ? (
            <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive">
              <span>{t('apiKeys.group.loadError')}</span>
              <Button type="button" size="sm" variant="secondary" disabled={props.loading} onClick={props.onRetry}>{t('apiKeys.actions.retry')}</Button>
            </div>
          ) : null}
          <p className="text-xs text-muted-foreground">{t('apiKeys.group.createHint')}</p>
        </>
      )}
    </ApiKeyField>
  )
}
