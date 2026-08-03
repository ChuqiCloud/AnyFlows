import { LoaderCircle, Rows3 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
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
  return (
    <div className="flex flex-col gap-3 border-y border-[var(--hairline)] py-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="grid min-w-0 flex-1 gap-1.5 sm:max-w-md">
        <Label htmlFor="credential-channel">{t('credentials.channel.label')}</Label>
        <Select id="credential-channel" value={props.selected?.id ?? ''} onChange={(event) => props.onSelect(Number(event.target.value))}>
          {props.channels.map((channel) => <option key={channel.id} value={channel.id}>{channel.name} · #{channel.id}</option>)}
        </Select>
      </div>
      <div className="flex min-h-9 flex-wrap items-center gap-2">
        {props.selected ? <><Badge>{t(`channels.type.${props.selected.type}`)}</Badge><Badge>{t(`channels.protocol.${props.selected.protocol}`)}</Badge></> : null}
        {props.hasNextPage ? (
          <Button type="button" size="sm" variant="ghost" disabled={props.loadingMore} onClick={props.onLoadMore}>
            {props.loadingMore ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Rows3 aria-hidden="true" />}
            {t('credentials.channel.loadMore')}
          </Button>
        ) : null}
      </div>
    </div>
  )
}
