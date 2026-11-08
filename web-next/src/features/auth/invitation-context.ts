export type InvitationContext =
  | { kind: 'none' }
  | { kind: 'valid'; code: string }
  | { kind: 'invalid' }

export function isValidInvitationCode(value: string) {
  return value.length === 25 && /^af-[A-Za-z0-9_-]{22}$/u.test(value)
}

/** Keep referral attribution in the current URL, without carrying it to later sessions. */
export function readInvitationContext(search: string | URLSearchParams): InvitationContext {
  const values = new URLSearchParams(search).getAll('invite')
  if (values.length === 0) return { kind: 'none' }
  if (values.length !== 1 || !isValidInvitationCode(values[0])) return { kind: 'invalid' }
  return { kind: 'valid', code: values[0] }
}

export function invitationAuthLink(path: '/login' | '/register', inviteCode?: string) {
  return inviteCode ? `${path}?invite=${encodeURIComponent(inviteCode)}` : path
}

export function oauthInvitationQuery(inviteCode?: string) {
  if (inviteCode === undefined) return undefined
  if (!isValidInvitationCode(inviteCode)) throw new Error('Invalid invitation code')
  return { invite_code: inviteCode }
}
