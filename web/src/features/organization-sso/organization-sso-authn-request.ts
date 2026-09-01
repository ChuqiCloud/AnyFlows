import type { OrganizationSsoLoginStartResponse } from '@/lib/api/generated/types.gen'

type RandomValues = (buffer: Uint8Array<ArrayBuffer>) => Uint8Array<ArrayBuffer>

/** 生成仅用于审计关联的 256 位启动请求摘要，不保存到浏览器存储。 */
export function createOrganizationSsoRequestDigest(
  randomValues: RandomValues = (buffer) => globalThis.crypto.getRandomValues(buffer),
) {
  const buffer = new Uint8Array(new ArrayBuffer(32))
  return Array.from(randomValues(buffer), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** 将服务端返回的受控 SAML POST 表单直接交给浏览器提交。 */
export function submitOrganizationSsoSamlAuthnRequest(response: OrganizationSsoLoginStartResponse) {
  const request = response.saml_authn_request
  if (!request || !isSafeAction(request.action) || !hasExpectedFields(request.fields)) return false

  const form = document.createElement('form')
  form.method = 'post'
  form.action = request.action
  form.hidden = true
  form.setAttribute('aria-hidden', 'true')
  for (const field of request.fields) {
    const input = document.createElement('input')
    input.type = 'hidden'
    input.name = field.name
    input.value = field.value
    form.append(input)
  }
  document.body.append(form)
  form.submit()
  window.setTimeout(() => form.remove(), 0)
  return true
}

function isSafeAction(value: string) {
  try {
    const url = new URL(value)
    return url.protocol === 'https:'
      && url.username === ''
      && url.password === ''
      && url.search === ''
      && url.hash === ''
  } catch {
    return false
  }
}

function hasExpectedFields(fields: readonly { name: string; value: string }[]) {
  if (fields.length !== 2) return false
  const names = fields.map((field) => field.name)
  return names.includes('SAMLRequest')
    && names.includes('RelayState')
    && fields.every((field) => field.value.length > 0)
}
