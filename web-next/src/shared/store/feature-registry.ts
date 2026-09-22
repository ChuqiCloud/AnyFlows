import type { Reducer } from '@reduxjs/toolkit'

/**
 * 业务模块的 Redux 分片注册表。
 *
 * AnyFlows 的业务域尚未迁移进模块体系，这里先保持为空：
 * 每迁移一个业务模块，就在此登记它的 reducer，并由该模块的 module.ts 声明。
 */
export interface FeatureStateRegistry {}

export type FeatureReducersMap = {
  [K in keyof FeatureStateRegistry]: Reducer<FeatureStateRegistry[K]>
}
