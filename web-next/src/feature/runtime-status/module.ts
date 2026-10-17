import { defineModule } from '@/shared/modules'
import { runtimeStatusNavNodes } from './navigation'

const runtimeStatusModule = defineModule({
  id: 'runtimeStatus',
  order: 44,
  navigation: runtimeStatusNavNodes,
})

export { runtimeStatusModule }
export default runtimeStatusModule
