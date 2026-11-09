export type MerchantServicePrice = { id: number; service_id: number; revision_id: number; effective_at: number; default_ratio_micros: number; model_ratios: Array<{ model: string; ratio_micros: number | null }>; status: 'scheduled' | 'active' | 'superseded' | 'cancelled' | 'invalidated'; reason: string | null; created_at: number; updated_at: number }
export type MerchantServicePriceWrite = { expected_revision_id: number; effective_at: number; default_ratio_micros: number; model_ratios: Array<{ model: string; ratio_micros: number | null }> }
export type MerchantMaintenanceWrite = { starts_at: number; ends_at: number; message: string; blocks_requests: boolean }
export type MerchantMaintenance = MerchantMaintenanceWrite & { id: number; service_id: number; status: 'scheduled' | 'active' | 'completed' | 'cancelled'; created_at: number; updated_at: number }
export type MerchantPayoutSettings = { version: number; enabled: boolean; auto_approve_enabled: boolean; gateway_url: string; currency: string; quota_per_unit: number; min_amount_minor: number; max_amount_minor: number; daily_limit_minor: number; signing_key_configured: boolean }
export type MerchantPayoutSettingsWrite = Omit<MerchantPayoutSettings, 'version' | 'signing_key_configured'> & { expected_version: number; signing_key?: string; clear_signing_key: boolean }
export type MerchantPayoutAccount = { configured: boolean; label: string; version: number }
export type MerchantPayout = { withdrawal_id: number; merchant_id: number; event_key: string; status: 'pending' | 'processing' | 'unknown' | 'paid' | 'failed'; amount_minor: number; currency: string; reference: string | null; reason: string | null; created_at: number; updated_at: number }

export function merchantLocalDateTime(seconds: number) { const date = new Date(seconds * 1000); return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16) }
export function merchantDateTimeSeconds(value: string) { const millis = new Date(value).getTime(); return value && Number.isFinite(millis) ? Math.floor(millis / 1000) : undefined }
