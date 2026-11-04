import { defineModule } from '@/shared/modules'
import { userRoutesNavNodes } from './navigation'

const userRoutesModule = defineModule({ id: 'userRoutes', order: 11, navigation: userRoutesNavNodes })
export { userRoutesModule }
export default userRoutesModule
