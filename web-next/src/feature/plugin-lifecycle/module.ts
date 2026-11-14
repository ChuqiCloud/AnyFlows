import { defineModule } from '@/shared/modules'
import { pluginLifecycleNavNodes } from './navigation'

const pluginLifecycleModule = defineModule({
  id: 'pluginLifecycle',
  order: 45,
  navigation: pluginLifecycleNavNodes,
})

export { pluginLifecycleModule }
export default pluginLifecycleModule
