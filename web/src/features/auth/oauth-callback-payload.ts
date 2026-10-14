export type OAuthCallbackPayload =
  | { kind: 'ticket'; ticket: string; method?: string }
  | { kind: 'cancelled' }
  | { kind: 'failed' }

const ticketPattern = /^[A-Za-z0-9_-]{43}$/
const methodPattern = /^[a-z0-9-]+:[a-z0-9-]+$/

export function parseOAuthCallbackPayload(search: string, hash: string): OAuthCallbackPayload {
  const hashQuery = hash.startsWith('#/oauth/callback?') ? hash.slice('#/oauth/callback?'.length) : null
  if (hashQuery !== null && search !== '') return { kind: 'failed' }
  const query = new URLSearchParams(hashQuery ?? search)
  const tickets = query.getAll('ticket')
  const errors = query.getAll('error')
  const methods = query.getAll('method')
  if (methods.length > 1 || (methods.length === 1 && !methodPattern.test(methods[0]))) return { kind: 'failed' }
  if (tickets.length === 1 && errors.length === 0 && ticketPattern.test(tickets[0])) {
    return { kind: 'ticket', ticket: tickets[0], ...(methods.length === 1 ? { method: methods[0] } : {}) }
  }
  if (tickets.length === 0 && errors.length === 1 && errors[0] === 'cancelled') return { kind: 'cancelled' }
  return { kind: 'failed' }
}
