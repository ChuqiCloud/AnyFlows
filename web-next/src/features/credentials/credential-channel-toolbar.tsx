import { Button, Chip, Select, SelectItem } from '@heroui/react'
import { LoaderCircle, Rows3 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminChannel } from '@/lib/api/generated/types.gen'

type CredentialChannelToolbarProps = {
  channels: readonly AdminChannel[]
  selected?: AdminChannel
  loadingMore: boolean
  hasNextPage: boolean
  onSelect: (channelId: number) => void
  onLoadMore: () => void
}

/** 账号池始终从真实渠道目录选择作用域，不把裸 ID 暴露为主要交互。 */
export function CredentialChannelToolbar(props: CredentialChannelToolbarProps) {
  const { t } = useTranslation()
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const channelItems = props.channels.map((channel) => ({ key: String(channel.id), label: `${channel.name} · #${channel.id}` }))
  return (
    <div className="flex flex-col gap-3 border-t border-[var(--hairline)] py-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="grid min-w-0 flex-1 gap-1.5 sm:max-w-md">
        <label className="text-xs font-medium leading-none text-foreground" htmlFor="credential-channel">{t('credentials.channel.label')}</label>
        <Select
          aria-label={t('credentials.channel.label')}
          id="credential-channel"
          items={channelItems}
          selectedKeys={props.selected ? [String(props.selected.id)] : []}
          size="sm"
          onSelectionChange={(keys) => {
            const next = Array.from(keys)[0]
            if (next !== undefined) props.onSelect(Number(next))
          }}
        >
          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
        </Select>
      </div>
      <div className="flex min-h-9 flex-wrap items-center gap-2">
        {props.selected ? <><Chip size="sm" variant="flat">{t(`channels.type.${props.selected.type}`)}</Chip><Chip size="sm" variant="flat">{t(`channels.protocol.${props.selected.protocol}`)}</Chip></> : null}
        {props.hasNextPage ? (
          <Button type="button" size="sm" variant="light" isDisabled={props.loadingMore} onClick={props.onLoadMore}>
            {props.loadingMore ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Rows3 className="size-3.5" aria-hidden="true" />}
            {t('credentials.channel.loadMore')}
          </Button>
        ) : null}
      </div>
    </div>
  )
}
