export type ExtensionRouteDefinition = {
  path: `/console/extensions/${string}`
  titleKey: string
  access: 'user' | 'admin'
  capabilities: readonly string[]
  entryPoint?: 'account-verification'
  navigation?: {
    group: { id: string; titleKey: string }
    icon?: 'building' | 'package' | 'clipboard' | 'shield' | 'settings'
    order?: number
  }
}

export function validateExtensionRoutes<T extends ExtensionRouteDefinition>(routes: readonly T[]): readonly T[] {
  const paths = new Set<string>()
  const entryPoints = new Set<string>()
  const groups = new Map<string, string>()
  for (const route of routes) {
    if (route.access !== 'user' && route.access !== 'admin') throw new Error('Invalid extension access: ' + route.path)
    if (!/^\/console\/extensions\/[a-z0-9-]+(?:\/[a-z0-9-]+)*$/.test(route.path)) {
      throw new Error('Invalid console extension route: ' + route.path)
    }
    if (paths.has(route.path)) throw new Error('Duplicate console extension route: ' + route.path)
    if (route.capabilities.length === 0) throw new Error('Missing extension capabilities: ' + route.path)
    if (route.entryPoint !== undefined) {
      if (route.entryPoint !== 'account-verification' || route.access !== 'user' || entryPoints.has(route.entryPoint)) {
        throw new Error('Invalid or duplicate extension entry point: ' + route.path)
      }
      entryPoints.add(route.entryPoint)
    }
    if (route.navigation) {
      const { group, icon, order } = route.navigation
      if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(group.id) || group.id === 'extensions' || !group.titleKey.trim()) {
        throw new Error('Invalid extension navigation group: ' + route.path)
      }
      if (groups.has(group.id) && groups.get(group.id) !== group.titleKey) throw new Error('Conflicting extension navigation group: ' + group.id)
      if (icon !== undefined && !['building', 'package', 'clipboard', 'shield', 'settings'].includes(icon)) throw new Error('Invalid extension navigation icon: ' + route.path)
      if (order !== undefined && !Number.isFinite(order)) throw new Error('Invalid extension navigation order: ' + route.path)
      groups.set(group.id, group.titleKey)
    }
    paths.add(route.path)
  }
  return routes
}

export function supportsExtensionRoute(route: ExtensionRouteDefinition, capabilities: ReadonlySet<string>) {
  return route.capabilities.every((capability) => capabilities.has(capability))
}

/** Group only routes already filtered by role and runtime capabilities. */
export function groupExtensionNavigation<T extends ExtensionRouteDefinition>(routes: readonly T[]) {
  const groups = new Map<string, { id: string; titleKey?: string; routes: T[] }>()
  for (const route of [...routes].sort((a, b) => (a.navigation?.order ?? 0) - (b.navigation?.order ?? 0))) {
    const id = route.navigation?.group.id ?? 'extensions'
    if (!groups.has(id)) groups.set(id, { id, titleKey: route.navigation?.group.titleKey, routes: [] })
    groups.get(id)!.routes.push(route)
  }
  return [...groups.values()]
}
