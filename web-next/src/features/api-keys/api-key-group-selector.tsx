import { useId } from 'react'
import { useTranslation } from 'react-i18next'

import { Button, Select, SelectItem } from '@heroui/react'
import type { UserToken } from '@/lib/api/generated/types.gen'
import { ApiKeyField } from './api-key-form-field'
import { formatGroupBaseRatio, type SelectableTokenGroup } from './api-key-group-model'

type ApiKeyGroupSelectorProps = {
  token?: UserToken
  portalContainer?: HTMLElement | null
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
  const labelId = useId()
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
          <span id={labelId} className="sr-only">{t('apiKeys.group.label')}</span>
          <Select
            aria-label={t('apiKeys.group.label')}
            aria-labelledby={labelId}
            popoverProps={props.portalContainer ? { portalContainer: props.portalContainer } : undefined}
            id="api-key-group"
            isDisabled={props.loading || props.failed || !props.groups}
            isInvalid={!!props.error}
            items={[
              { key: '__default__', label: defaultGroup ? t('apiKeys.group.followOption', { group: groupLabel(defaultGroup) }) : t('apiKeys.group.followDefault') },
              ...(props.groups ?? []).map((group) => ({ key: String(group.id), label: groupLabel(group) })),
            ]}
            selectedKeys={[props.value || '__default__']}
            size="sm"
            disallowEmptySelection
            onSelectionChange={(keys) => {
              const key = Array.from(keys)[0]
              if (key !== undefined) props.onChange(key === '__default__' ? '' : String(key))
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
          {props.loading ? <p role="status" className="text-xs text-muted-foreground">{t('apiKeys.group.loading')}</p> : null}
          {props.failed ? (
            <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive">
              <span>{t('apiKeys.group.loadError')}</span>
              <Button type="button" size="sm" variant="flat" isDisabled={props.loading} onClick={props.onRetry}>{t('apiKeys.actions.retry')}</Button>
            </div>
          ) : null}
          <p className="text-xs text-muted-foreground">{t('apiKeys.group.createHint')}</p>
        </>
      )}
    </ApiKeyField>
  )
}
