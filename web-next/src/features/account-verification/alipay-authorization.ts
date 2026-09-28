// Authorization URLs are capabilities. Keep them in memory and encode QR codes locally.
export type AlipayFlow = {
  provider: string
  status: number
  provider_status?: string | null
  provider_action_url?: string | null
  provider_expires_at?: number | null
  server_time?: number
}

export function alipayAuthorizationUrl(value?: string | null): string | null {
  if (!value) return null
  try {
    const url = new URL(value)
    if (url.origin !== 'https://openauth.alipay.com' || url.username || url.password
      || url.pathname !== '/oauth2/publicAppAuthorize.htm' || url.hash
      || url.searchParams.get('scope') !== 'id_verify'
      || !/^[a-f0-9]{32}$/i.test(url.searchParams.get('state') ?? '')) return null
    return url.href
  } catch { return null }
}

export function alipayLaunchUrl(value: string, userAgent: string): string | null {
  const url = alipayAuthorizationUrl(value)
  if (!url) return null
  return /AlipayClient/i.test(userAgent) ? url
    : `alipays://platformapi/startapp?appId=20000067&url=${encodeURIComponent(url)}`
}

export function isAlipayMobile(userAgent: string): boolean {
  return /Android|iPhone|iPad|iPod|AlipayClient/i.test(userAgent)
}

export function alipayRemainingSeconds(flow: AlipayFlow, receivedAt: number, now: number): number {
  if (flow.provider !== 'alipay' || flow.status !== 1 || flow.provider_status === 'expired'
    || !Number.isFinite(flow.provider_expires_at) || !Number.isFinite(flow.server_time)) return 0
  // Use the server clock; a phone with an incorrect date must not extend a flow.
  const lifetime = (flow.provider_expires_at! - flow.server_time!) * 1000
  return Math.max(0, Math.ceil((lifetime - Math.max(0, now - receivedAt)) / 1000))
}
