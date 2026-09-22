/** 前端统一 API 错误，保留状态码、请求标识与结构化详情。 */
export class ApiError extends Error {
  readonly status: number | undefined
  readonly details: unknown
  readonly requestId: string | undefined

  constructor(
    message: string,
    options: {
      cause?: unknown
      details?: unknown
      requestId?: string
      status?: number
    } = {},
  ) {
    super(message, { cause: options.cause })
    this.name = 'ApiError'
    this.status = options.status
    this.details = options.details
    this.requestId = options.requestId
  }

  get isClientError() {
    return this.status !== undefined && this.status >= 400 && this.status < 500
  }

  /** 将生成客户端或网络层的未知错误收敛为稳定错误类型。 */
  static from(error: unknown, response?: Response) {
    const details = error
    const message = extractMessage(error, response)

    return new ApiError(message, {
      cause: error,
      details,
      requestId: response?.headers.get('x-request-id') ?? undefined,
      status: response?.status,
    })
  }
}

function extractMessage(error: unknown, response?: Response) {
  if (error instanceof Error && error.message) {
    return error.message
  }

  if (typeof error === 'string' && error.length > 0) {
    return error
  }

  if (isRecord(error)) {
    if (typeof error.message === 'string' && error.message.length > 0) {
      return error.message
    }

    if (isRecord(error.error) && typeof error.error.message === 'string') {
      return error.error.message
    }
  }

  if (response) {
    return `请求失败（HTTP ${response.status}）`
  }

  return '网络请求失败'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}
