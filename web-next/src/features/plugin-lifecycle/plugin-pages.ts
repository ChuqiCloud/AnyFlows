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
    if (!page || Object.keys(page).some((key) => !['id', 'schemaVersion', 'title', 'description', 'capabilityId', 'capabilityVersion', 'input'].includes(key))
      || typeof page.id !== 'string' || !/^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$/u.test(page.id)
      || page.schemaVersion !== 1
      || typeof page.capabilityId !== 'string'
      || typeof page.capabilityVersion !== 'number' || !Number.isInteger(page.capabilityVersion)
      || page.capabilityVersion < 1 || page.capabilityVersion > 65535) return []
    const encoded = JSON.stringify(page)
    if (!encoded || new TextEncoder().encode(encoded).length > 8 * 1024) return []
    const declaration = capabilities.map(record).find((item) => item?.id === page.capabilityId && item.version === page.capabilityVersion)
    const isReadOnly = declaration?.access === 'read' && declaration.sideEffect === 'none'
    const granted = deployment.grantedCapabilities.some((grant) => grant.id === page.capabilityId && grant.version === page.capabilityVersion)
    const title = localizedPluginUiText(page.title, locale)
    const description = page.description === undefined ? undefined : localizedPluginUiText(page.description, locale)
    if (!isReadOnly || !granted || !title || (page.description !== undefined && !description)) return []
    return [{
      id: page.id,
      schemaVersion: 1,
      title,
      ...(description ? { description } : {}),
      capabilityId: page.capabilityId,
      capabilityVersion: page.capabilityVersion,
      input: page.input ?? {},
      inputSchema: declaration?.inputSchema ?? {},
    }]
  })
}
