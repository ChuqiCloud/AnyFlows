function errorStatus(error: unknown) {
  return typeof error === 'object' && error !== null && 'status' in error
    ? (error as { status?: unknown }).status
    : undefined
}

/** 将创建错误收敛为不会泄露响应正文的分享界面分类。 */
export function classifyShareMutationError(error: unknown) {
  const status = errorStatus(error)
  if (status === 400) return 'invalid'
  if (status === 409) return 'limit'
  return 'unavailable'
}

/** 只有服务端明确返回 404 时才显示分享失效，其余故障允许重试。 */
export function classifyPublicShareError(error: unknown) {
  return errorStatus(error) === 404 ? 'not_found' : 'unavailable'
}
