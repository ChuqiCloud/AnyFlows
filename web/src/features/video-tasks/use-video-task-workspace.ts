import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  clearPlaygroundApiKey,
  getPlaygroundApiKey,
  isPlaygroundApiKey,
  setPlaygroundApiKey,
} from '@/features/playground/playground-api-key'
import { pollVideoTask, submitVideoTask, videoTaskError } from './video-task-client'
import type { VideoTaskApiError, VideoTaskPollResponse, VideoTaskRequest } from './video-task-types'

const POLL_INTERVAL_MS = 3_000

export type VideoTaskWorkspacePhase =
  | 'idle'
  | 'submitting'
  | 'pending'
  | 'polling'
  | 'done'
  | 'expired'
  | 'failed'
  | 'error'

/** 管理单个可安全重放的视频任务工作区，禁止自动重试付费提交。 */
export function useVideoTaskWorkspace() {
  const { t } = useTranslation()
  const [apiKey, setApiKey] = useState(() => getPlaygroundApiKey() ?? '')
  const [apiKeyDraft, setApiKeyDraft] = useState(apiKey)
  const [apiKeyError, setApiKeyError] = useState<string>()
  const [idempotencyKey, setIdempotencyKey] = useState(generateIdempotencyKey)
  const [taskId, setTaskId] = useState<string>()
  const [phase, setPhase] = useState<VideoTaskWorkspacePhase>('idle')
  const [response, setResponse] = useState<VideoTaskPollResponse>()
  const [error, setError] = useState<VideoTaskApiError>()
  const [requestLocked, setRequestLocked] = useState(false)
  const controllerRef = useRef<AbortController | null>(null)

  const saveApiKey = useCallback(() => {
    if (!isPlaygroundApiKey(apiKeyDraft)) {
      setApiKeyError(t('videoTasks.apiKey.invalid'))
      return
    }
    const normalized = apiKeyDraft.trim()
    setPlaygroundApiKey(normalized)
    setApiKey(normalized)
    setApiKeyDraft(normalized)
    setApiKeyError(undefined)
  }, [apiKeyDraft, t])

  const clearApiKey = useCallback(() => {
    clearPlaygroundApiKey()
    setApiKey('')
    setApiKeyDraft('')
    setApiKeyError(undefined)
  }, [])

  const submit = useCallback(async (request: VideoTaskRequest) => {
    if (!apiKey) {
      setApiKeyError(t('videoTasks.apiKey.required'))
      return
    }
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setRequestLocked(true)
    setPhase('submitting')
    setError(undefined)
    try {
      const nextTaskId = await submitVideoTask(
        apiKey,
        idempotencyKey,
        request,
        controller.signal,
      )
      setTaskId(nextTaskId)
      setResponse({ status: 'pending' })
      setPhase('pending')
    } catch (cause) {
      const detail = videoTaskError(cause)
      if (detail.code === 'cancelled') return
      setError(detail)
      setPhase('error')
    }
  }, [apiKey, idempotencyKey, t])

  const poll = useCallback(async () => {
    if (!apiKey || !taskId) return
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setPhase('polling')
    setError(undefined)
    try {
      const nextResponse = await pollVideoTask(apiKey, taskId, controller.signal)
      setResponse(nextResponse)
      setPhase(nextResponse.status)
    } catch (cause) {
      const detail = videoTaskError(cause)
      if (detail.code === 'cancelled') return
      setError(detail)
      setPhase('error')
    }
  }, [apiKey, taskId])

  useEffect(() => {
    if (phase !== 'pending' || !taskId || !apiKey) return
    const timer = window.setTimeout(() => void poll(), POLL_INTERVAL_MS)
    return () => window.clearTimeout(timer)
  }, [apiKey, phase, poll, taskId])

  useEffect(() => () => controllerRef.current?.abort(), [])

  const startNew = useCallback(() => {
    controllerRef.current?.abort()
    setIdempotencyKey(generateIdempotencyKey())
    setTaskId(undefined)
    setPhase('idle')
    setResponse(undefined)
    setError(undefined)
    setRequestLocked(false)
  }, [])

  const busy = phase === 'submitting' || phase === 'polling'
  const canRetrySubmission = phase === 'error'
    && error?.code === 'request_outcome_unknown'
    && taskId === undefined
  const canPoll = Boolean(taskId && apiKey && !busy)

  return useMemo(() => ({
    apiKey,
    apiKeyDraft,
    apiKeyError,
    busy,
    canPoll,
    canRetrySubmission,
    clearApiKey,
    error,
    idempotencyKey,
    phase,
    poll,
    requestLocked,
    response,
    saveApiKey,
    setApiKeyDraft: (value: string) => {
      setApiKeyDraft(value)
      setApiKeyError(undefined)
    },
    startNew,
    submit,
    taskId,
  }), [
    apiKey,
    apiKeyDraft,
    apiKeyError,
    busy,
    canPoll,
    canRetrySubmission,
    clearApiKey,
    error,
    idempotencyKey,
    phase,
    poll,
    requestLocked,
    response,
    saveApiKey,
    startNew,
    submit,
    taskId,
  ])
}

function generateIdempotencyKey() {
  const bytes = new Uint8Array(16)
  do {
    crypto.getRandomValues(bytes)
  } while (bytes.every((value) => value === 0))
  return Array.from(bytes, (value) => value.toString(16).padStart(2, '0')).join('')
}
