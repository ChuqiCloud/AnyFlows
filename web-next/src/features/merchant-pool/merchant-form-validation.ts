export type MerchantTextError = 'required' | 'textBytes' | 'singleLine'

/** Match the backend's UTF-8 and control-character limits on submitted text. */
export function merchantTextError(value: string, maxBytes: number, required = true): MerchantTextError | undefined {
  const submitted = value.trim()
  if (required && submitted.length === 0) return 'required'
  if (/\p{Cc}/u.test(submitted)) return 'singleLine'
  if (new TextEncoder().encode(submitted).byteLength > maxBytes) return 'textBytes'
  return undefined
}
