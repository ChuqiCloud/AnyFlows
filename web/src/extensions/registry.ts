import { lazy, type ComponentType } from 'react'

import { i18n } from '@/i18n'
import { validateExtensionRoutes, type ExtensionRouteDefinition } from './model'

export type ConsoleExtensionRoute = ExtensionRouteDefinition & {
  load: () => Promise<{ default: ComponentType }>
}

export type ConsoleExtension = {
  id: string
  routes: readonly ConsoleExtensionRoute[]
  resources: Record<string, Record<string, unknown>>
}

const modules = import.meta.glob<{ default: ConsoleExtension }>('./modules/*/index.ts', { eager: true })
const extensions = Object.values(modules).map((module) => module.default)
const ids = new Set<string>()
for (const extension of extensions) {
  if (!/^[a-z0-9-]+$/.test(extension.id) || ids.has(extension.id)) {
    throw new Error('Invalid or duplicate console extension: ' + extension.id)
  }
  ids.add(extension.id)
  for (const [language, resource] of Object.entries(extension.resources)) {
    i18n.addResourceBundle(language, extension.id, resource)
  }
}

export const consoleExtensionRoutes = validateExtensionRoutes(
  extensions.flatMap((extension) => extension.routes),
).map((route) => ({ ...route, Component: lazy(route.load) }))

export function findConsoleExtensionRoute(path: string) {
  return consoleExtensionRoutes.find((route) => route.path === path)
}
