import type {
  VideoFailureCode as GeneratedVideoFailureCode,
  VideoGenerationRequest,
  VideoPollResponse,
  VideoTaskListItem as GeneratedVideoTaskListItem,
  VideoTaskListResponse as GeneratedVideoTaskListResponse,
  VideoTaskStatus as GeneratedVideoTaskStatus,
} from '@/lib/api/generated/types.gen'

/** 页面直接复用 OpenAPI 生成类型，避免视频协议字段在前后端漂移。 */
export type VideoTaskRequest = VideoGenerationRequest
export type VideoTaskPollResponse = VideoPollResponse
export type VideoTaskStatus = GeneratedVideoTaskStatus
export type VideoFailureCode = GeneratedVideoFailureCode

export type VideoTaskApiError = {
  code: string
  message: string
  status: number
}

export type VideoTaskListItem = GeneratedVideoTaskListItem
export type VideoTaskListPage = Omit<GeneratedVideoTaskListResponse, 'next_cursor'> & {
  next_cursor?: string
}
