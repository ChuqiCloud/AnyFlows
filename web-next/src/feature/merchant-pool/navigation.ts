import { defineNavigation } from '@/shared/navigation'

export const merchantPoolNavNodes = defineNavigation([
  { key: 'merchantPool', title: 'merchantPool.title', href: '/console/merchant-pool', order: 12, section: 'personal', sectionTitle: 'nav.groups.personal', surface: 'user' },
  { key: 'merchantWorkspace', title: 'merchantPool.workspace', href: '/console/merchant', order: 13, section: 'personal', sectionTitle: 'nav.groups.personal', surface: 'user' },
  { key: 'merchantPoolAdmin', title: 'merchantPool.adminTitle', href: '/console/system-settings/merchant-pool', order: 36, section: 'gateway', sectionTitle: 'nav.sections.gateway', surface: 'admin' },
], { group: 'module', module: 'merchantPool' })
