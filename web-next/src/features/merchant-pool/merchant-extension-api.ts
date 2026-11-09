import { useQuery } from '@tanstack/react-query'
import { merchantGet } from './merchant-pool-api'
import type { MerchantCursorList, PoolPage } from './merchant-pool-model'
import type { MerchantMaintenance, MerchantPayout, MerchantPayoutAccount, MerchantPayoutSettings, MerchantServicePrice } from './merchant-extension-model'

export function merchantServicePath(serviceId: number, admin: boolean) { return admin ? `/api/admin/merchant-services/${serviceId}` : `/api/merchant/services/${serviceId}` }
export function useMerchantServicePrices(serviceId: number, admin: boolean, page: number) { return useQuery({ queryKey: ['merchant-pool', 'service-prices', serviceId, admin, page], queryFn: ({ signal }) => merchantGet<PoolPage<MerchantServicePrice>>(`${merchantServicePath(serviceId, admin)}/prices`, signal, { page, page_size: 20 }) }) }
export function useMerchantMaintenance(serviceId: number, admin: boolean, page: number) { return useQuery({ queryKey: ['merchant-pool', 'maintenance', serviceId, admin, page], queryFn: ({ signal }) => merchantGet<PoolPage<MerchantMaintenance>>(`${merchantServicePath(serviceId, admin)}/maintenance`, signal, { page, page_size: 20 }) }) }
export function useMerchantPayoutSettings() { return useQuery({ queryKey: ['merchant-pool', 'payout-settings'], queryFn: ({ signal }) => merchantGet<MerchantPayoutSettings>('/api/admin/merchant-payout-settings', signal) }) }
export function useMerchantPayoutAccount() { return useQuery({ queryKey: ['merchant-pool', 'payout-account'], queryFn: ({ signal }) => merchantGet<MerchantPayoutAccount>('/api/merchant/payout-account', signal) }) }
export function useMerchantPayouts(admin = false, beforeId?: string) { return useQuery({ queryKey: ['merchant-pool', 'payouts', admin, beforeId], queryFn: ({ signal }) => merchantGet<MerchantCursorList<MerchantPayout>>(admin ? '/api/admin/merchant-payouts' : '/api/merchant/payouts', signal, { limit: 20, before_id: beforeId }), refetchInterval: 30_000, refetchIntervalInBackground: false }) }
