import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  bindAdminChannelClientProfile, getAdminChannelClientProfile,
  getAdminClientProfile, getAdminClientProfileCapabilities, getAdminClientProfileRevision,
  getAdminClientProfileOpenCodeSettings, listAdminClientProfiles,
  listAdminClientProfileReferences, listAdminClientProfileRevisions,
  saveAdminClientProfile, setAdminClientProfileEnabled,
  setAdminClientProfileOpenCodeEntry, validateAdminClientProfile,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannel, ClientProfileDefinitionSchema, ClientProfileRecordSchema,
  ClientProfileBindingSchema, ClientProfileServiceSchema,
} from '@/lib/api/generated/types.gen'

export const clientProfilesKey = ['client-profiles'] as const
export type ProfileFilters = {
  search?: string
  enabled?: boolean
  service?: ClientProfileServiceSchema
  channel_type?: AdminChannel['type']
  protocol?: AdminChannel['protocol']
}

export function useProfileCapabilities() {
  return useQuery({
    queryKey: [...clientProfilesKey, 'capabilities'], staleTime: 60_000,
    queryFn: async ({ signal }) => (await getAdminClientProfileCapabilities({ client: apiClient, signal })).data,
  })
}

export function useProfiles(filters: ProfileFilters, after?: string) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'list', filters, after],
    queryFn: async ({ signal }) => (await listAdminClientProfiles({
      client: apiClient, query: { ...filters, after, limit: 25 }, signal,
    })).data,
  })
}

export async function readProfile(key: string, signal?: AbortSignal) {
  const { data: record } = await getAdminClientProfile({ client: apiClient, path: { key }, signal })
  const { data: snapshot } = await getAdminClientProfileRevision({
    client: apiClient, path: { key, revision: record.latest_revision }, signal,
  })
  return { record, snapshot }
}

export function useProfileDetail(key?: string) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'detail', key], enabled: !!key,
    gcTime: 0, refetchOnWindowFocus: false, refetchOnReconnect: false,
    queryFn: ({ signal }) => readProfile(key!, signal),
  })
}

export const writeProfile = async (key: string, definition: ClientProfileDefinitionSchema, expected_version: number) =>
  (await saveAdminClientProfile({ client: apiClient, path: { key }, body: { definition, expected_version } })).data
export const validateProfile = async (definition: ClientProfileDefinitionSchema, expected_version: number) =>
  (await validateAdminClientProfile({ client: apiClient, body: { definition, expected_version } })).data
export const toggleProfile = async (record: ClientProfileRecordSchema) =>
  (await setAdminClientProfileEnabled({ client: apiClient, path: { key: record.key }, body: { enabled: !record.enabled, expected_version: record.version } })).data

export function useProfileRevisions(key: string, before?: number, enabled = true) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'revisions', key, before], enabled,
    queryFn: async ({ signal }) => (await listAdminClientProfileRevisions({ client: apiClient, path: { key }, query: { before, limit: 25 }, signal })).data,
  })
}

export function useProfileReferences(key: string, after?: number, enabled = true) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'references', key, after], enabled,
    queryFn: async ({ signal }) => (await listAdminClientProfileReferences({ client: apiClient, path: { key }, query: { after, limit: 25 }, signal })).data,
  })
}

export function useChannelProfile(channelId: number) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'binding', channelId], gcTime: 0,
    refetchOnWindowFocus: false, refetchOnReconnect: false,
    queryFn: async ({ signal }) => (await getAdminChannelClientProfile({ client: apiClient, path: { channel_id: channelId }, signal })).data,
  })
}

export const bindProfile = async (channelId: number, binding: ClientProfileBindingSchema, expected_version: number) =>
  (await bindAdminChannelClientProfile({ client: apiClient, path: { channel_id: channelId }, body: { binding, expected_version } })).data

export function useOpenCodeSettings(channelId: number, enabled: boolean) {
  return useQuery({
    queryKey: [...clientProfilesKey, 'opencode', channelId], enabled, gcTime: 0,
    refetchOnWindowFocus: false, refetchOnReconnect: false,
    queryFn: async ({ signal }) => (await getAdminClientProfileOpenCodeSettings({ client: apiClient, path: { channel_id: channelId }, signal })).data,
  })
}

export const setOpenCodeEntry = async (channelId: number, version: string, entry: 'inference_v1' | 'zen_v1', full_compatibility: boolean) =>
  (await setAdminClientProfileOpenCodeEntry({ client: apiClient, path: { channel_id: channelId }, body: { version, entry, full_compatibility } })).data
