import { useEffect, useState } from 'react'
import { ApiError } from '@/lib/api'
import type { ClientProfileDefinitionSchema } from '@/lib/api/generated/types.gen'

export function useProfileSearch() {
  const [search, setSearch] = useState('')
  const [debounced, setDebounced] = useState('')
  useEffect(() => {
    const timer = window.setTimeout(() => setDebounced(search.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [search])
  return { search, setSearch, debounced }
}

export function useCursorPage<T>() {
  const [cursors, setCursors] = useState<(T | undefined)[]>([undefined])
  return {
    cursor: cursors.at(-1), page: cursors.length,
    reset: () => setCursors([undefined]),
    previous: () => setCursors(current => current.length > 1 ? current.slice(0, -1) : current),
    next: (cursor: T) => setCursors(current => [...current, cursor]),
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

export function validProfileDraft(key: string, definition: ClientProfileDefinitionSchema) {
  return /^[a-z0-9][a-z0-9_-]{0,63}$/.test(key)
    && definition.name.trim().length > 0
    && definition.protocols.length > 0 && definition.credential_kinds.length > 0
    && Object.values(definition.identity_headers).every(value => value.trim().length > 0)
}
