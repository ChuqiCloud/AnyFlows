import { defineNavigation } from '@/shared/navigation'

export const pluginLifecycleNavNodes = defineNavigation([
  { key: 'pluginLifecycle', title: 'nav.pluginLifecycle', href: '/console/plugin-lifecycle', order: 45 },
], { group: 'module', module: 'pluginLifecycle', surface: 'admin' })
