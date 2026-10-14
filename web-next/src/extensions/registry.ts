import { lazy, type ComponentType, type LazyExoticComponent, type ReactNode } from 'react'

import { i18n } from '@/i18n'
import type { LoginResponse } from '@/lib/api/generated/types.gen'
import { validateExtensionRoutes, type ExtensionRouteDefinition } from './model'

export type ConsoleExtensionRoute = ExtensionRouteDefinition & {
  load: () => Promise<{ default: ComponentType }>
}

export type ExtensionLoginMethodProps = {
  onBack: () => void
  registrationAvailable: boolean
}

export type ExtensionLoginMethod = {
  id: string
  labelKey: string
  descriptionKey?: string
  icon: ReactNode
  exchangeTicket?: (ticket: string, context: Readonly<Record<string, string>>) => Promise<LoginResponse>
  load: () => Promise<{ default: ComponentType<ExtensionLoginMethodProps> }>
}

export type RegisteredExtensionLoginMethod = ExtensionLoginMethod & {
  namespace: string
  Component: LazyExoticComponent<ComponentType<ExtensionLoginMethodProps>>
}

export type ConsoleExtension = {
  id: string
  routes: readonly ConsoleExtensionRoute[]
  resources: Record<string, Record<string, unknown>>
  loginMethods?: readonly ExtensionLoginMethod[]
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

const loginMethodIds = new Set<string>()
for (const extension of extensions) {
  for (const method of extension.loginMethods ?? []) {
    const id = `${extension.id}:${method.id}`
    if (!/^[a-z0-9-]+$/.test(method.id) || loginMethodIds.has(id) || !method.labelKey.trim()) {
      throw new Error('Invalid or duplicate extension login method: ' + id)
    }
    loginMethodIds.add(id)
  }
}

export const extensionLoginMethods: readonly RegisteredExtensionLoginMethod[] = extensions.flatMap((extension) =>
  (extension.loginMethods ?? []).map((method) => ({
    ...method,
    namespace: extension.id,
    Component: lazy(method.load),
  })),
)

export const consoleExtensionRoutes = validateExtensionRoutes(
  extensions.flatMap((extension) => extension.routes),
).map((route) => ({ ...route, Component: lazy(route.load) }))

export function findConsoleExtensionRoute(path: string) {
  return consoleExtensionRoutes.find((route) => route.path === path)
}
