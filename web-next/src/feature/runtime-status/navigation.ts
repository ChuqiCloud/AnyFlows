import { defineNavigation } from '@/shared/navigation'

export const runtimeStatusNavNodes = defineNavigation([
  { key: 'runtimeStatus', title: 'nav.runtimeStatus', href: '/console/runtime-status', order: 44 },
], { group: 'module', module: 'runtimeStatus' })
