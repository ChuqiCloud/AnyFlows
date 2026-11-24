import { Button } from '@heroui/react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { apiClient } from '@/lib/api'
import { recoverAdminRoutingHealth } from '@/lib/api/generated/sdk.gen'

import { adminChannelQueryKey, adminChannelsQueryKey } from './channel-api'

export function RoutingHealthRecoveryButton({ channelId, credentialId, reenable }: {
  channelId: number
  credentialId?: number
  reenable?: boolean
}) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const mutation = useMutation({
    mutationFn: async () => {
      const { data } = await recoverAdminRoutingHealth({
        client: apiClient,
        path: { channel_id: channelId },
        body: { credential_id: credentialId },
      })
      return data
    },
    onSuccess: async (result) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['admin-channel-credentials', channelId, 'health'] }),
        ...(result.channel_reenabled ? [
          queryClient.invalidateQueries({ queryKey: adminChannelsQueryKey }),
          queryClient.invalidateQueries({ queryKey: adminChannelQueryKey(channelId) }),
        ] : []),
      ])
    },
  })
  const result = mutation.data
  const feedback = mutation.isError ? 'failed'
    : result?.channel_reenabled ? 'enabled'
      : result?.probe_requested ? 'requested'
        : mutation.isSuccess ? 'unavailable' : undefined

  return (
    <div className="grid justify-items-start gap-1">
      <Button type="button" size="sm" variant="light" aria-busy={mutation.isPending}
        isDisabled={mutation.isPending} onPress={() => mutation.mutate()}>
        <RotateCcw className={mutation.isPending ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
        {t(reenable ? 'channels.recovery.enable' : 'channels.recovery.probe')}
      </Button>
      {feedback ? <span className={mutation.isError ? 'text-destructive' : 'text-muted-foreground'}
        role={mutation.isError ? 'alert' : 'status'}>{t('channels.recovery.' + feedback)}</span> : null}
    </div>
  )
}
