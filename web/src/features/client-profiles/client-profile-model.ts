import { useEffect, useState } from 'react'
import { ApiError } from '@/lib/api'
import type { ClientProfileDefinitionSchema } from '@/lib/api/generated/types.gen'
import { scopedCursors } from './client-profile-policy'

export function useProfileSearch() {
  const [search, setSearch] = useState('')
  const [debounced, setDebounced] = useState('')
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(search.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [search])
  return { search, setSearch, debounced }
}

export function useCursorPage<T>(scope = '') {
  const [state, setState] = useState<{ scope: string; cursors: (T | undefined)[] }>({ scope, cursors: [undefined] })
  const cursors = scopedCursors(state, scope)
  return {
    cursor: cursors.at(-1), page: cursors.length,
    previous: () => setState({ scope, cursors: cursors.length > 1 ? cursors.slice(0, -1) : cursors }),
    next: (cursor: T) => setState({ scope, cursors: [...cursors, cursor] }),
  }
}

export function profileErrorKey(error: unknown) {
  if (error instanceof ApiError) {
    if (error.status === 409) return 'clientProfiles.conflict'
    if (error.status === 403) return 'clientProfiles.forbidden'
    if (error.status === 400) return 'clientProfiles.invalid'
  }
  return 'clientProfiles.failed'
}

export function profileLogo(definition?: ClientProfileDefinitionSchema, service?: string) {
  const icons: Record<string, string> = { standard: 'OpenAI', claude_code: 'Anthropic', codex: 'OpenAI', open_code: 'OpenCode' }
  return definition?.icon ?? icons[service ?? definition?.service ?? 'standard']
}

export function isProfileAccessError(error: unknown) {
  return error instanceof ApiError && (error.status === 401 || error.status === 403)
}
