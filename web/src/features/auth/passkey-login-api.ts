import { apiClient } from '@/lib/api'
import type { LoginResponse } from '@/lib/api/generated/types.gen'

type PasskeyAuthenticationOptionsResponse = {
  options: unknown
}

export type PasskeyClientErrorCode = 'unsupported' | 'cancelled' | 'invalid_credential'

export class PasskeyClientError extends Error {
  readonly code: PasskeyClientErrorCode

  constructor(code: PasskeyClientErrorCode) {
    super(code)
    this.name = 'PasskeyClientError'
    this.code = code
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function base64UrlToBuffer(value: string): ArrayBuffer {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=')
  const decoded = atob(padded)
  const bytes = new Uint8Array(decoded.length)
  for (let index = 0; index < decoded.length; index += 1) bytes[index] = decoded.charCodeAt(index)
  return bytes.buffer
}

function bufferToBase64Url(value: ArrayBuffer): string {
  const bytes = new Uint8Array(value)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '')
}

/** 将服务端 JSON 中的 Base64URL 字段恢复为 WebAuthn API 所需的二进制值。 */
function toRequestOptions(response: PasskeyAuthenticationOptionsResponse): PublicKeyCredentialRequestOptions {
  const value = response.options
  const root = isRecord(value) && isRecord(value.publicKey)
    ? value.publicKey
    : isRecord(value)
      ? value
      : {}
  const options: Record<string, unknown> = { ...root }
  if (typeof options.challenge === 'string') options.challenge = base64UrlToBuffer(options.challenge)
  if (Array.isArray(options.allowCredentials)) {
    options.allowCredentials = options.allowCredentials.map((credential: unknown) => (
      isRecord(credential) && typeof credential.id === 'string'
        ? { ...credential, id: base64UrlToBuffer(credential.id) }
        : credential
    ))
  }
  return options as unknown as PublicKeyCredentialRequestOptions
}

/** 仅序列化 WebAuthn 断言协议字段，不读取或缓存设备侧私钥材料。 */
function serializeAuthenticationCredential(credential: PublicKeyCredential): Record<string, unknown> {
  const response = credential.response
  if (!(response instanceof AuthenticatorAssertionResponse)) {
    throw new PasskeyClientError('invalid_credential')
  }
  return {
    id: credential.id,
    rawId: bufferToBase64Url(credential.rawId),
    type: credential.type,
    response: {
      authenticatorData: bufferToBase64Url(response.authenticatorData),
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      signature: bufferToBase64Url(response.signature),
      userHandle: response.userHandle ? bufferToBase64Url(response.userHandle) : null,
    },
  }
}

/** 检查当前浏览器是否具备执行 Passkey 认证的最低 API。 */
export function supportsPasskeyAuthentication() {
  return typeof window !== 'undefined'
    && 'PublicKeyCredential' in window
    && typeof navigator.credentials?.get === 'function'
}

/** 完成用户名优先的 Passkey options、浏览器认证和 verify 三段流程。 */
export async function authenticateWithPasskey(username: string): Promise<LoginResponse> {
  if (!supportsPasskeyAuthentication()) throw new PasskeyClientError('unsupported')

  const { data: options } = await apiClient.post<
    { 200: PasskeyAuthenticationOptionsResponse },
    unknown,
    true
  >({
    body: { username },
    headers: { 'Content-Type': 'application/json' },
    throwOnError: true,
    url: '/api/auth/passkey/options',
  })

  let credential: Credential | null
  try {
    credential = await navigator.credentials.get({ publicKey: toRequestOptions(options) })
  } catch (error) {
    if (error instanceof DOMException && (error.name === 'AbortError' || error.name === 'NotAllowedError')) {
      throw new PasskeyClientError('cancelled')
    }
    throw error
  }
  if (!(credential instanceof PublicKeyCredential)) {
    throw new PasskeyClientError('invalid_credential')
  }

  const { data: session } = await apiClient.post<{ 200: LoginResponse }, unknown, true>({
    body: { credential: serializeAuthenticationCredential(credential) },
    headers: { 'Content-Type': 'application/json' },
    throwOnError: true,
    url: '/api/auth/passkey/verify',
  })
  return session
}
