export type ExtensionRouteDefinition = {
  path: `/console/extensions/${string}`
  titleKey: string
  access: 'user' | 'admin'
  capabilities: readonly string[]
}

export function validateExtensionRoutes<T extends ExtensionRouteDefinition>(routes: readonly T[]): readonly T[] {
  const paths = new Set<string>()
  for (const route of routes) {
    if (route.access !== 'user' && route.access !== 'admin') throw new Error('Invalid extension access: ' + route.path)
    if (!/^\/console\/extensions\/[a-z0-9-]+(?:\/[a-z0-9-]+)*$/.test(route.path)) {
      throw new Error('Invalid console extension route: ' + route.path)
    }
    if (paths.has(route.path)) throw new Error('Duplicate console extension route: ' + route.path)
    if (route.capabilities.length === 0) throw new Error('Missing extension capabilities: ' + route.path)
    paths.add(route.path)
  }
  return routes
}

export function supportsExtensionRoute(route: ExtensionRouteDefinition, capabilities: ReadonlySet<string>) {
  return route.capabilities.every((capability) => capabilities.has(capability))
}
