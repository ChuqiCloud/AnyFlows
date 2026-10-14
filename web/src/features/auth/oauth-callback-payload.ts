export type OAuthCallbackPayload =
  | { kind: 'ticket'; ticket: string; method?: string; context: Readonly<Record<string, string>> }
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
  const context: Record<string, string> = {}
  let invalidContext = false
  query.forEach((value, key) => {
    if (key === 'ticket' || key === 'error' || key === 'method') return
    if (!/^[a-z][a-z0-9_]{0,63}$/.test(key) || value.length > 4096 || key in context) {
      invalidContext = true
      return
    }
    context[key] = value
  })
  if (invalidContext) return { kind: 'failed' }
  if (tickets.length === 1 && errors.length === 0 && ticketPattern.test(tickets[0])) {
    return { kind: 'ticket', ticket: tickets[0], ...(methods.length === 1 ? { method: methods[0] } : {}), context }
  }
  if (tickets.length === 0 && errors.length === 1 && errors[0] === 'cancelled' && Object.keys(context).length === 0) return { kind: 'cancelled' }
  return { kind: 'failed' }
}
