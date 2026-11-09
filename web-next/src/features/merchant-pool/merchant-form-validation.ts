export type MerchantTextError = 'required' | 'textBytes' | 'singleLine'

/** Match backend UTF-8 and control-character limits; multiline text accepts LF and CRLF. */
export function merchantTextError(value: string, maxBytes: number, required = true, multiline = false): MerchantTextError | undefined {
  const submitted = value.trim()
  if (required && submitted.length === 0) return 'required'
  const controls = multiline ? submitted.replace(/\r\n|\n/g, '') : submitted
  if (/\p{Cc}/u.test(controls)) return 'singleLine'
  if (new TextEncoder().encode(submitted).byteLength > maxBytes) return 'textBytes'
  return undefined
}

export function merchantDescriptionError(value: string, maxBytes: number, required = false): MerchantTextError | undefined {
  return merchantTextError(value, maxBytes, required, true)
}
