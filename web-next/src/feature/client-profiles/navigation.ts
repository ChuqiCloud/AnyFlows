import { defineNavigation } from '@/shared/navigation'
export const clientProfilesNavNodes = defineNavigation([
  { key: 'clientProfiles', title: 'nav.clientProfiles', href: '/console/client-profiles', order: 31, section: 'gateway', sectionTitle: 'nav.sections.gateway' },
], { group: 'module', module: 'clientProfiles' })
