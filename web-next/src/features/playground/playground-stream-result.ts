export type PlaygroundResolvedStreamResult = {
  finishReason: string
  interrupted: boolean
  usage: {
    inputTokens?: number
    outputTokens?: number
    totalTokens?: number
  }
}

/**
 * 已产生正文后若终态帧失败，返回可展示的部分响应；没有正文时仍保留原始错误。
 * 这条边界只降级展示状态，不把截断响应伪装成完整成功。
 */
export async function resolvePlaygroundStreamResult(input: {
  finishReason: PromiseLike<string>
  streamError: unknown
  textReceived: boolean
  usage: PromiseLike<PlaygroundResolvedStreamResult['usage']>
}): Promise<PlaygroundResolvedStreamResult> {
  if (input.streamError && !input.textReceived) throw input.streamError

  const [usageResult, finishReasonResult] = await Promise.allSettled([
    input.usage,
    input.finishReason,
  ])
  const metadataInterrupted = usageResult.status === 'rejected'
    || finishReasonResult.status === 'rejected'
  if (metadataInterrupted && !input.textReceived) {
    throw usageResult.status === 'rejected'
      ? usageResult.reason
      : (finishReasonResult as PromiseRejectedResult).reason
  }

  return {
    finishReason: finishReasonResult.status === 'fulfilled'
      ? finishReasonResult.value
      : 'unknown',
    interrupted: Boolean(input.streamError) || metadataInterrupted,
    usage: usageResult.status === 'fulfilled' ? usageResult.value : {},
  }
}
