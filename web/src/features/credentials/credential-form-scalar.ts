const MICROS = 1_000_000n
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER)

export function validInteger(value: string, minimum: number, maximum: number) {
  if (!/^-?\d+$/.test(value)) return false
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed >= minimum && parsed <= maximum
}

/** 将最多六位小数精确换算为百万分整数，不经过浮点运算。 */
export function parseMicros(value: string) {
  const match = /^(\d+)(?:\.(\d{1,6}))?$/.exec(value)
  if (!match) return undefined
  const result = BigInt(match[1]) * MICROS + BigInt((match[2] ?? '').padEnd(6, '0'))
  return result <= MAX_SAFE_INTEGER ? Number(result) : undefined
}

/** 将百万分整数无损还原为紧凑十进制字符串。 */
export function formatMicros(value?: number | null) {
  if (value === undefined || value === null) return ''
  if (!Number.isSafeInteger(value) || value < 0) return ''
  const micros = BigInt(value)
  const fraction = (micros % MICROS).toString().padStart(6, '0').replace(/0+$/, '')
  return fraction === '' ? (micros / MICROS).toString() : `${micros / MICROS}.${fraction}`
}

export function validSimpleSecret(value: string) {
  return value.length > 0 && value.length <= 16 * 1_024 && value.trim() === value
    && /^[\x20-\x7E]+$/.test(value)
}

export function validOAuthToken(value: string) {
  return value.length > 0 && value.length <= 16 * 1_024 && /^[\x21-\x7E]+$/.test(value)
}

export function validAsciiComponent(value: string, maximum: number) {
  return value.length > 0 && value.length <= maximum && /^[\x21-\x7E]+$/.test(value)
}

export function validServiceAccountEmail(value: string) {
  if (!validAsciiComponent(value, 320)) return false
  const parts = value.split('@')
  if (parts.length !== 2 || !/^[a-z0-9.-]+$/.test(parts[0])) return false
  const labels = parts[1].split('.')
  return parts[1].endsWith('.gserviceaccount.com')
    && labels.every((label) => /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/.test(label))
}

/** 浏览器只做 PEM 结构校验，RSA 密钥解析仍由服务端权威完成。 */
export function validPrivateKeyPem(value: string) {
  const asciiPem = Array.from(value).every((character) => {
    const code = character.charCodeAt(0)
    return code === 0x0A || code === 0x0D || (code >= 0x20 && code <= 0x7E)
  })
  return value.length > 0 && value.length <= 16 * 1_024 && asciiPem
    && /^-----BEGIN (?:RSA )?PRIVATE KEY-----[\s\S]+-----END (?:RSA )?PRIVATE KEY-----\r?\n?$/.test(value)
}

export function validOptionalText(value: string, maximum: number) {
  return value === '' || (value.length <= maximum && value.trim() === value
    && Array.from(value).every((character) => {
      const code = character.charCodeAt(0)
      return code >= 0x20 && code !== 0x7F
    }))
}

export function optionalText(value: string) { return value === '' ? null : value }
export function optionalInteger(value: string) { return value === '' ? null : Number(value) }
export function optionalMicros(value: string) { return value === '' ? null : parseMicros(value) ?? null }
export function optionalNumber(value?: number | null) { return value === undefined || value === null ? '' : String(value) }
