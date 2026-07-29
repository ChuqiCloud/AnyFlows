import type { PlaygroundOverallState } from './playground-types'

export type PlaygroundDisplayStatus = PlaygroundOverallState
  | 'ready'
  | 'loadingModels'
  | 'modelUnavailable'
  | 'keyRequired'

/** 返回统一的状态色，避免总状态与模型列使用不同语义。 */
export function playgroundStatusClassName(status: PlaygroundDisplayStatus) {
  if (status === 'error' || status === 'modelUnavailable') {
    return 'bg-destructive/12 text-destructive'
  }
  if (status === 'partial' || status === 'cancelled' || status === 'keyRequired') {
    return 'bg-warning/12 text-warning'
  }
  if (status === 'streaming' || status === 'loadingModels') {
    return 'bg-info/12 text-info'
  }
  return 'bg-success/12 text-success'
}
