import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

export type ActiveMonitorSettings = { enabled: boolean; global_daily_budget: number }
export type ActiveMonitorTarget = {
  id: number; channel_id: number; credential_id: number; model: string; enabled: boolean
  interval_secs: number; daily_budget: number; timeout_secs: number; alert_after_secs: number; daily_used: number
  last_attempt_epoch_seconds: number | null; last_success_epoch_seconds: number | null
  failure_since_epoch_seconds: number | null; alert_active: boolean; muted_until_epoch_seconds: number | null
  is_muted: boolean
}
export type ActiveMonitorEvent = { id: number; target_id: number; kind: 'down' | 'recovered'; latency_ms: number | null; occurred_epoch_seconds: number }
export type ActiveMonitorTargetWrite = Omit<ActiveMonitorTarget, 'id' | 'daily_used' | 'last_attempt_epoch_seconds' | 'last_success_epoch_seconds' | 'failure_since_epoch_seconds' | 'alert_active' | 'is_muted'>

const key = ['active-channel-monitor'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export function useActiveMonitorSettings() {
  return useQuery({ queryKey: [...key, 'settings'], queryFn: async ({ signal }) => (await apiClient.get<{ 200: ActiveMonitorSettings }, unknown, true>({ url: '/api/admin/channel-monitor/settings', security, signal })).data })
}

export function useActiveMonitorTargets() {
  return useQuery({
    queryKey: [...key, 'targets'],
    refetchInterval: 30_000,
    queryFn: async ({ signal }) => {
      const targets = (await apiClient.get<{ 200: Omit<ActiveMonitorTarget, 'is_muted'>[] }, unknown, true>({ url: '/api/admin/channel-monitor/targets', security, signal })).data
      const now = Math.floor(Date.now() / 1000)
      return targets.map((target) => ({ ...target, is_muted: target.muted_until_epoch_seconds !== null && target.muted_until_epoch_seconds > now }))
    },
  })
}

export function useActiveMonitorEvents(targetId: number, enabled: boolean) {
  return useQuery({ queryKey: [...key, 'events', targetId], enabled, queryFn: async ({ signal }) => (await apiClient.get<{ 200: ActiveMonitorEvent[] }, unknown, true>({ url: '/api/admin/channel-monitor/targets/{id}/events', path: { id: targetId }, security, signal })).data })
}

export function useActiveMonitorMutation() {
  const client = useQueryClient()
  const refresh = async () => client.invalidateQueries({ queryKey: key })
  const settings = useMutation({ mutationFn: async (body: ActiveMonitorSettings) => (await apiClient.patch<{ 200: ActiveMonitorSettings }, unknown, true>({ url: '/api/admin/channel-monitor/settings', body, headers: { 'Content-Type': 'application/json' }, security })).data, onSuccess: refresh })
  const saveTarget = useMutation({ mutationFn: async (body: ActiveMonitorTargetWrite) => (await apiClient.post<{ 200: ActiveMonitorTarget }, unknown, true>({ url: '/api/admin/channel-monitor/targets', body, headers: { 'Content-Type': 'application/json' }, security })).data, onSuccess: refresh })
  const deleteTarget = useMutation({ mutationFn: async (id: number) => (await apiClient.delete<{ 204: void }, unknown, true>({ url: '/api/admin/channel-monitor/targets/{id}', path: { id }, security })).data, onSuccess: refresh })
  return { settings, saveTarget, deleteTarget }
}
