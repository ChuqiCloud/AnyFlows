export const PLAYGROUND_BOTTOM_THRESHOLD_PX = 72

export type PlaygroundScrollMetrics = {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
}

/** 判断对话视口是否仍贴近底部，给触控回弹保留少量像素余量。 */
export function isPlaygroundTranscriptNearBottom(
  metrics: PlaygroundScrollMetrics,
  threshold = PLAYGROUND_BOTTOM_THRESHOLD_PX,
) {
  const bottomGap = metrics.scrollHeight - metrics.scrollTop - metrics.clientHeight
  return bottomGap <= Math.max(0, threshold)
}
