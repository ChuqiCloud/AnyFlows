import type { AdminChannel, ClientProfileDefinitionSchema } from '@/lib/api/generated/types.gen'

export function compatibleProfile(definition: ClientProfileDefinitionSchema, channel: Pick<AdminChannel, 'type' | 'protocol'>) {
  if (!definition.protocols.includes(channel.protocol)) return false
  return definition.service === 'standard'
    || (definition.service === 'claude_code' && channel.type === 'anthropic')
    || (definition.service === 'codex' && channel.type === 'openai')
    || (definition.service === 'open_code' && channel.type === 'opencode')
}

export function validProfileDraft(key: string, definition: ClientProfileDefinitionSchema) {
  return /^[a-z0-9][a-z0-9_-]{0,63}$/.test(key)
    && definition.name.trim().length > 0
    && definition.protocols.length > 0 && definition.credential_kinds.length > 0
    && Object.values(definition.identity_headers).every(value => value.trim().length > 0)
}

/** 游标只能在生成它的筛选条件下复用。 */
export function scopedCursors<T>(state: { scope: string; cursors: (T | undefined)[] }, scope: string): (T | undefined)[] {
  return state.scope === scope ? state.cursors : [undefined]
}
