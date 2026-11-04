import { defineNavigation } from '@/shared/navigation'

export const userRoutesNavNodes = defineNavigation([{
  key: 'userRoutes', title: 'nav.userRoutes', href: '/console/user-routes', order: 11, section: 'personal', sectionTitle: 'nav.groups.personal',
}], { group: 'module', module: 'userRoutes', surface: 'user' })
