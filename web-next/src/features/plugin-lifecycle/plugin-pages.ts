import type { PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import { localizedPluginUiText } from './plugin-ui-views'

export type PluginPageDefinition = {
  id: string
  schemaVersion: 1
  title: string
  description?: string
  capabilityId: string
  capabilityVersion: number
  input: unknown
  inputSchema: unknown
  actions: PluginPageActionDefinition[]
}

export type PluginPageActionDefinition = {
  id: string
  schemaVersion: 1
  title: string
  description?: string
  capabilityId: string
  capabilityVersion: number
  input: unknown
  inputSchema: unknown
}

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

/** Return only host-routed pages bound to capabilities granted by this live deployment. */
export function pluginPages(deployment: PluginDeploymentResponse, locale: string): PluginPageDefinition[] {
  if (!deployment.enabled || !deployment.artifact.approval || deployment.artifact.revocation) return []
  const manifest = record(deployment.artifact.manifest)
  if (!Array.isArray(manifest?.uiPages) || manifest.uiPages.length > 32) return []
  const capabilities = Array.isArray(manifest.capabilities) ? manifest.capabilities : []
  return manifest.uiPages.flatMap((pageValue): PluginPageDefinition[] => {
    const page = record(pageValue)
    if (!page || Object.keys(page).some((key) => !['id', 'schemaVersion', 'title', 'description', 'capabilityId', 'capabilityVersion', 'input', 'actions'].includes(key))
      || typeof page.id !== 'string' || !/^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$/u.test(page.id)
      || page.schemaVersion !== 1
      || typeof page.capabilityId !== 'string'
      || typeof page.capabilityVersion !== 'number' || !Number.isInteger(page.capabilityVersion)
      || page.capabilityVersion < 1 || page.capabilityVersion > 65535) return []
    const encoded = JSON.stringify(page)
    if (!encoded || new TextEncoder().encode(encoded).length > 8 * 1024) return []
    const declaration = capabilities.map(record).find((item) => item !== undefined && item.id === page.capabilityId && item.version === page.capabilityVersion)
    const isReadOnly = declaration?.access === 'read' && declaration.sideEffect === 'none'
    const granted = deployment.grantedCapabilities.some((grant) => grant.id === page.capabilityId && grant.version === page.capabilityVersion)
    const title = localizedPluginUiText(page.title, locale)
    const description = page.description === undefined ? undefined : localizedPluginUiText(page.description, locale)
    if (!isReadOnly || !granted || !title || (page.description !== undefined && !description)) return []
    const actions = parsePluginPageActions(page.actions, capabilities, deployment.grantedCapabilities, locale)
    return [{
      id: page.id,
      schemaVersion: 1,
      title,
      ...(description ? { description } : {}),
      capabilityId: page.capabilityId,
      capabilityVersion: page.capabilityVersion,
      input: page.input ?? {},
      inputSchema: declaration?.inputSchema ?? {},
      actions,
    }]
  })
}

function parsePluginPageActions(
  value: unknown,
  capabilities: unknown[],
  grants: PluginDeploymentResponse['grantedCapabilities'],
  locale: string,
): PluginPageActionDefinition[] {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.length > 16) return []
  const ids = new Set<string>()
  return value.flatMap((actionValue): PluginPageActionDefinition[] => {
    const action = record(actionValue)
    if (!action || Object.keys(action).some((key) => !['id', 'schemaVersion', 'title', 'description', 'capabilityId', 'capabilityVersion', 'input'].includes(key))
      || typeof action.id !== 'string' || !/^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$/u.test(action.id)
      || ids.has(action.id)
      || action.schemaVersion !== 1
      || typeof action.capabilityId !== 'string'
      || typeof action.capabilityVersion !== 'number' || !Number.isInteger(action.capabilityVersion)
      || action.capabilityVersion < 1 || action.capabilityVersion > 65535) return []
    const encoded = JSON.stringify(action)
    if (!encoded || new TextEncoder().encode(encoded).length > 8 * 1024) return []
    const declaration = capabilities.map(record).find((item) => item !== undefined && item.id === action.capabilityId && item.version === action.capabilityVersion)
    const semantics = record(declaration?.writeSemantics)
    const writable = declaration?.access === 'write'
      && declaration.sideEffect === 'external_write'
      && semantics?.nativeIdempotencyKey === true
    const granted = grants.some((grant) => grant.id === action.capabilityId && grant.version === action.capabilityVersion)
    const title = localizedPluginUiText(action.title, locale)
    const description = action.description === undefined ? undefined : localizedPluginUiText(action.description, locale)
    if (!writable || !granted || !title || (action.description !== undefined && !description)) return []
    ids.add(action.id)
    return [{
      id: action.id,
      schemaVersion: 1,
      title,
      ...(description ? { description } : {}),
      capabilityId: action.capabilityId,
      capabilityVersion: action.capabilityVersion,
      input: action.input ?? {},
      inputSchema: declaration?.inputSchema ?? {},
    }]
  })
}
