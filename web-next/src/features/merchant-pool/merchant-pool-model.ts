import type { MerchantMaintenance, MerchantPayout, MerchantServicePrice } from './merchant-extension-model'
import type { UserRouteStrategy } from '../user-routes/user-route-model'

export type PoolPage<T> = { items: T[]; total: number; page: number; page_size: number }
export type MerchantPool = { id: number; group_id: number; name: string; description: string; enabled: boolean; commission_bps: number; commission_inherit_global: boolean; effective_commission_bps: number; min_ratio_micros: number; max_ratio_micros: number; settlement_delay_secs: number; created_at: number; updated_at: number }
export type MerchantPoolWrite = Omit<MerchantPool, 'id' | 'created_at' | 'updated_at' | 'effective_commission_bps'>
export type MerchantProfile = { id: number; owner_user_id: number; name: string; description: string; status: 'pending' | 'approved' | 'rejected' | 'suspended'; review_reason: string | null; created_at: number; updated_at: number }
export type MerchantPrices = { currency: 'USD'; unit: 'million_tokens' | 'second'; billing_mode: 'token' | 'free' | 'video'; video_per_second: { p480: string; p720: string; p1080: string | null } | null; input: string; output: string; cache_read: string; cache_write_5m: string; cache_write_1h: string; version: number }
export type MerchantQuote = { models: Array<{ model: string; sales_ratio_micros: number; base_prices: MerchantPrices; prices: MerchantPrices; net_prices: MerchantPrices }>; effective_commission_bps: number }
export type MerchantModelWrite = { model: string; upstream_model: string; ratio_micros: number | null; context_window: number | null; max_output_tokens: number | null; supports_streaming: boolean; supports_tools: boolean; supports_vision: boolean }
export type MerchantModelOffer = Omit<MerchantModelWrite, 'upstream_model' | 'ratio_micros'> & { provider: string | null; sales_ratio_micros: number; prices: MerchantPrices | null }
export type MerchantModelProvider = { name: string | null; model_count: number }
export type MerchantCatalogService = { id: number; pool_id: number; merchant_id: number; merchant_name: string; name: string; description: string; status: string; revision: number; price_version: number; maintenance: MerchantMaintenance[]; protocol: MerchantProtocol; models: MerchantModelOffer[]; updated_at: number }
export type MerchantServiceWrite = { pool_id: number; name: string; description: string; base_url: string; protocol: MerchantProtocol; api_key: string | null; default_ratio_micros: number; models: MerchantModelWrite[]; concurrency: number | null; timeout_secs: number }
export type MerchantRevision = Omit<MerchantServiceWrite, 'api_key' | 'pool_id'> & { id: number; revision: number; status: 'draft' | 'pending' | 'published' | 'rejected' | 'superseded'; has_api_key: boolean; review_reason: string | null; created_at: number; updated_at: number }
export type MerchantOwnedService = { id: number; merchant_id: number; pool_id: number; name: string; description: string; status: 'draft' | 'pending' | 'published' | 'paused' | 'suspended' | 'rejected'; active_price: MerchantServicePrice | null; maintenance: MerchantMaintenance[]; published: MerchantRevision | null; draft: MerchantRevision | null; created_at: number; updated_at: number }
export type MerchantPoolRoute = { id: number; name: string; pool_id: number; strategy: UserRouteStrategy; enabled: boolean; max_merchant_attempts: number; max_price_ratio_micros: number | null; members: Array<{ merchant_id: number; priority: number; weight: number }>; excluded_merchant_ids: number[]; allow_pool_fallback: boolean; accept_new_merchants: boolean }
export type MerchantRoutePreview = { items: Array<{ merchant_id: number; merchant_name: string; service_id: number; service_name: string; sales_ratio_micros: number; estimated_quota: number | null; eligible: boolean; reason: string | null }>; quota_per_usd: number; as_of: number }
export type MerchantRoutePreviewRequest = { route: MerchantPoolRouteWrite; model: string; protocol: MerchantProtocol; streaming: boolean; input_tokens: number; output_tokens: number; duration_seconds?: number; resolution?: '480p' | '720p' | '1080p' }
export type MerchantPoolRouteWrite = Omit<MerchantPoolRoute, 'id'>
export type MerchantStatistics = { service_id: number; merchant_id: number; sample_count: number; success_count: number; failure_count: number; success_rate: number | null; ttft_p50_ms: number | null; ttft_p95_ms: number | null; latency_p50_ms: number | null; latency_p95_ms: number | null; status: string; updated_at: number }
export type MerchantStatisticsResponse = { items: MerchantStatistics[]; as_of: number; window_seconds: number }
export type MerchantTimelineBucket = { started_at: number; ended_at: number; success_count: number; failure_count: number; sample_count: number; status: 'normal' | 'degraded' | 'unknown' }
export type MerchantTimeline = { service_id: number; merchant_id: number; buckets: MerchantTimelineBucket[] }
export type MerchantTimelineResponse = { items: MerchantTimeline[]; as_of: number; window_seconds: number }
export type MerchantFinance = { merchant_id: number; pending_quota: number; available_quota: number; withdrawing_quota: number; frozen_quota: number; debt_quota: number; total_gross_quota: number; total_commission_quota: number; total_paid_quota: number; quota_per_usd: number }
export type MerchantFinanceEntry = { id: number; kind: string; reference: string; gross_quota: number; commission_quota: number; net_quota: number; created_at: number }
export type MerchantFinanceReportRow = { period: string; service_id: number | null; service_name: string; model: string; gross_quota: number; commission_quota: number; net_quota: number; refunded_gross_quota: number; refunded_commission_quota: number; refunded_net_quota: number }
export type MerchantFinanceReport = { granularity: 'day' | 'month'; from_date: string; to_date: string; totals: { sales_gross_quota: number; commission_quota: number; merchant_net_quota: number; refunded_gross_quota: number; refunded_commission_quota: number; refunded_net_quota: number }; rows: MerchantFinanceReportRow[] }
export type MerchantWithdrawal = { payout: MerchantPayout | null; id: number; event_key: string; merchant_id: number; amount_quota: number; status: 'pending' | 'paid' | 'rejected'; reference: string | null; reason: string | null; created_at: number; updated_at: number }
export type MerchantCursorList<T> = { items: T[]; next_cursor: string | null }

export function parsePoolRatio(value: string): number | undefined {
  if (!/^(?:0|[1-9]\d*)(?:\.\d{1,6})?$/.test(value)) return undefined
  const [whole, fraction = ''] = value.split('.')
  const result = Number(whole) * 1_000_000 + Number(fraction.padEnd(6, '0'))
  return Number.isSafeInteger(result) && result >= 0 ? result : undefined
}
export function poolRatio(value: number) { return String(value / 1_000_000) }
export function poolEventKey() { return crypto.randomUUID().replaceAll('-', '') }
export function positiveInteger(value: string): number | undefined { if (!/^[1-9]\d*$/.test(value)) return undefined; const result = Number(value); return Number.isSafeInteger(result) ? result : undefined }
export function poolTimestamp(timestamp: number | null | undefined, locale: string) { return timestamp ? new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' }).format(timestamp * 1000) : '—' }
export function merchantWithdrawalAmount(value: string, unitsPerDisplayUnit: string): number | undefined {
  if (!/^(?:0|[1-9]\d*)(?:\.\d{1,6})?$/.test(value) || !/^[1-9]\d*$/.test(unitsPerDisplayUnit)) return undefined
  const [whole, fraction = ''] = value.split('.')
  const scale = 10n ** BigInt(fraction.length)
  const numerator = (BigInt(whole) * scale + BigInt(fraction || '0')) * BigInt(unitsPerDisplayUnit)
  if (numerator % scale !== 0n) return undefined
  const result = Number(numerator / scale)
  return Number.isSafeInteger(result) && result > 0 ? result : undefined
}

export const merchantProtocols = [{ value: 'openai_chat', label: 'OpenAI Chat Completions' }, { value: 'openai_responses', label: 'OpenAI Responses' }, { value: 'openai_images', label: 'OpenAI Images' }, { value: 'openai_audio', label: 'OpenAI Audio Transcription' }, { value: 'openai_speech', label: 'OpenAI Speech' }, { value: 'xai_video', label: 'xAI Video' }] as const
export type MerchantProtocol = typeof merchantProtocols[number]['value']
export function merchantProtocolLabel(protocol: string) { return merchantProtocols.find((item) => item.value === protocol)?.label ?? protocol }
export function merchantTextProtocol(protocol: string) { return protocol === 'openai_chat' || protocol === 'openai_responses' }
