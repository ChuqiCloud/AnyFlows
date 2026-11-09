import { defineModule } from '@/shared/modules'
import { merchantPoolNavNodes } from './navigation'

const merchantPoolModule = defineModule({ id: 'merchantPool', order: 12, navigation: merchantPoolNavNodes })
export { merchantPoolModule }
export default merchantPoolModule
