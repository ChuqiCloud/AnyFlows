import type { MerchantModelWrite, MerchantPool, MerchantProtocol, MerchantServiceWrite } from './merchant-pool-model'
import { merchantTextProtocol, parsePoolRatio, poolRatio, positiveInteger } from './merchant-pool-model'
import { merchantDescriptionError, merchantTextError } from './merchant-form-validation'

export const MAX_MERCHANT_MODELS = 128
export const MAX_MERCHANT_MODEL_TOKENS = 4_294_967_295

export type MerchantModelDraft = { rowId: string; model: string; upstream: string; ratio: string; context: string; maxOutput: string; streaming: boolean; tools: boolean; vision: boolean }
export type MerchantFieldError = { key: string; values?: Record<string, string | number> }
export type MerchantServiceErrors = Record<string, MerchantFieldError>
type Validation<T> = { value: T; errors: undefined } | { value: undefined; errors: MerchantServiceErrors }
type ModelFields = { pool: string; ratio: string; models: MerchantModelDraft[]; protocol: MerchantProtocol; pools: MerchantPool[] }
type QuotedFields = Pick<MerchantServiceWrite, 'pool_id' | 'default_ratio_micros' | 'models'>
export type MerchantServiceFields = ModelFields & { name: string; description: string; baseUrl: string; apiKey: string; concurrency: string; timeout: string; keyRequired: boolean }

export function merchantModelField(rowId: string, field: string) { return `model.${rowId}.${field}` }

function fieldError(key: string, values?: Record<string, string | number>): MerchantFieldError { return { key: `merchantPool.validation.${key}`, values } }

function textError(errors: MerchantServiceErrors, field: string, value: string, max: number, required = true) {
  const error = merchantTextError(value, max, required)
  if (error) errors[field] = fieldError(error, { max })
}

function optionalInteger(errors: MerchantServiceErrors, field: string, value: string, max: number): number | null {
  if (value.trim() === '') return null
  const parsed = positiveInteger(value.trim())
  if (parsed === undefined || parsed > max) {
    errors[field] = fieldError('positiveIntegerRange', { max })
    return null
  }
  return parsed
}

function ratioValue(errors: MerchantServiceErrors, field: string, value: string, pool?: MerchantPool): number | undefined {
  const parsed = parsePoolRatio(value.trim())
  if (parsed === undefined || parsed <= 0) {
    errors[field] = fieldError('ratioPrecision')
    return undefined
  }
  const min = Math.max(1, pool?.min_ratio_micros ?? 1)
  const max = Math.min(100_000_000, pool?.max_ratio_micros ?? 100_000_000)
  if (parsed < min || parsed > max) errors[field] = fieldError('ratioRange', { min: poolRatio(min), max: poolRatio(max) })
  return parsed
}

/** Quote and save use the same model payload; invalid numbers never become omitted limits. */
export function validateMerchantServiceModels(fields: ModelFields): Validation<QuotedFields> {
  const errors: MerchantServiceErrors = {}
  const poolId = positiveInteger(fields.pool)
  const selectedPool = fields.pools.find((pool) => pool.id === poolId)
  if (!poolId || !selectedPool?.enabled) errors.pool = fieldError('required')
  const defaultRatio = ratioValue(errors, 'ratio', fields.ratio, selectedPool)
  if (fields.models.length < 1 || fields.models.length > MAX_MERCHANT_MODELS) errors.models = fieldError('modelCount', { max: MAX_MERCHANT_MODELS })
  const names = new Map<string, string>()
  const models: MerchantModelWrite[] = fields.models.map((draft) => {
    const field = (name: string) => merchantModelField(draft.rowId, name)
    const model = draft.model.trim(), upstream = draft.upstream.trim() || model
    textError(errors, field('model'), model, 255)
    textError(errors, field('upstream'), upstream, 255)
    const duplicate = names.get(model)
    if (model && duplicate !== undefined) {
      errors[field('model')] = fieldError('duplicateModel')
      errors[merchantModelField(duplicate, 'model')] = fieldError('duplicateModel')
    } else names.set(model, draft.rowId)
    const video = model === 'grok-imagine-video' || model === 'grok-imagine-video-1.5' || model.startsWith('grok-imagine-video-1.5-')
    if (model && (fields.protocol === 'xai_video') !== video) errors[field('model')] = fieldError('videoModel')
    if (video && upstream !== model) errors[field('upstream')] = fieldError('videoModelMapping')
    const ratio = draft.ratio.trim() === '' ? null : ratioValue(errors, field('ratio'), draft.ratio, selectedPool)
    const context = optionalInteger(errors, field('context'), draft.context, MAX_MERCHANT_MODEL_TOKENS)
    const maxOutput = optionalInteger(errors, field('maxOutput'), draft.maxOutput, MAX_MERCHANT_MODEL_TOKENS)
    if (context !== null && maxOutput !== null && maxOutput > context) errors[field('maxOutput')] = fieldError('outputExceedsContext')
    return { model, upstream_model: upstream, ratio_micros: ratio ?? null, context_window: context, max_output_tokens: maxOutput, supports_streaming: merchantTextProtocol(fields.protocol) && draft.streaming, supports_tools: merchantTextProtocol(fields.protocol) && draft.tools, supports_vision: merchantTextProtocol(fields.protocol) && draft.vision }
  })
  if (Object.keys(errors).length > 0 || poolId === undefined || defaultRatio === undefined) return { value: undefined, errors }
  return { value: { pool_id: poolId, default_ratio_micros: defaultRatio, models }, errors: undefined }
}

export function validateMerchantService(fields: MerchantServiceFields): Validation<MerchantServiceWrite> {
  const modelResult = validateMerchantServiceModels(fields)
  const errors: MerchantServiceErrors = { ...modelResult.errors }
  textError(errors, 'name', fields.name, 128)
  const descriptionError = merchantDescriptionError(fields.description, 2048, false)
  if (descriptionError) errors.description = fieldError(descriptionError, { max: 2048 })
  textError(errors, 'apiKey', fields.apiKey, 8192, fields.keyRequired)
  const baseUrl = fields.baseUrl.trim()
  let validUrl = false
  try {
    const url = new URL(baseUrl)
    validUrl = url.protocol === 'https:' && (!url.port || url.port === '443') && !url.username && !url.password && !baseUrl.includes('?') && !baseUrl.includes('#') && !/\p{Cc}/u.test(baseUrl) && new TextEncoder().encode(baseUrl).byteLength <= 2048
  } catch { /* Invalid addresses remain available for correction. */ }
  if (!validUrl) errors.baseUrl = fieldError('httpsEndpoint')
  const concurrency = optionalInteger(errors, 'concurrency', fields.concurrency, 10_000)
  const timeout = optionalInteger(errors, 'timeout', fields.timeout, 300)
  if (timeout === null) errors.timeout = fieldError('positiveIntegerRange', { max: 300 })
  if (Object.keys(errors).length > 0 || modelResult.value === undefined || timeout === null) return { value: undefined, errors }
  return { value: { ...modelResult.value, name: fields.name.trim(), description: fields.description.trim(), base_url: baseUrl, protocol: fields.protocol, api_key: fields.apiKey.trim() || null, concurrency, timeout_secs: timeout }, errors: undefined }
}
