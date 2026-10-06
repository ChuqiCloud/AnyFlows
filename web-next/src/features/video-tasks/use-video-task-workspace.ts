import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

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
  const [idempotencyKey, setIdempotencyKey] = useState(generateIdempotencyKey)
  const [taskId, setTaskId] = useState<string>()
  const [phase, setPhase] = useState<VideoTaskWorkspacePhase>('idle')
  const [response, setResponse] = useState<VideoTaskPollResponse>()
  const [error, setError] = useState<VideoTaskApiError>()
  const [requestLocked, setRequestLocked] = useState(false)
  const controllerRef = useRef<AbortController | null>(null)

  const resetTask = useCallback(() => {
    controllerRef.current?.abort()
    setIdempotencyKey(generateIdempotencyKey())
    setTaskId(undefined)
    setPhase('idle')
    setResponse(undefined)
    setError(undefined)
    setRequestLocked(false)
  }, [])

  const submit = useCallback(async (request: VideoTaskRequest) => {
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setRequestLocked(true)
    setPhase('submitting')
    setError(undefined)
    try {
      const nextTaskId = await submitVideoTask(
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
  }, [idempotencyKey])

  const poll = useCallback(async () => {
    if (!taskId) return
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setPhase('polling')
    setError(undefined)
    try {
      const nextResponse = await pollVideoTask(taskId, controller.signal)
      setResponse(nextResponse)
      setPhase(nextResponse.status)
    } catch (cause) {
      const detail = videoTaskError(cause)
      if (detail.code === 'cancelled') return
      setError(detail)
      setPhase('error')
    }
  }, [taskId])

  const openTask = useCallback(async (nextTaskId: string) => {
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setTaskId(nextTaskId)
    setRequestLocked(true)
    setResponse(undefined)
    setPhase('polling')
    setError(undefined)
    try {
      const nextResponse = await pollVideoTask(nextTaskId, controller.signal)
      setResponse(nextResponse)
      setPhase(nextResponse.status)
    } catch (cause) {
      const detail = videoTaskError(cause)
      if (detail.code === 'cancelled') return
      setError(detail)
      setPhase('error')
    }
  }, [])

  useEffect(() => {
    if (phase !== 'pending' || !taskId) return
    const timer = window.setTimeout(() => void poll(), POLL_INTERVAL_MS)
    return () => window.clearTimeout(timer)
  }, [phase, poll, taskId])

  useEffect(() => () => controllerRef.current?.abort(), [])

  const startNew = useCallback(() => {
    resetTask()
  }, [resetTask])

  const busy = phase === 'submitting' || phase === 'polling'
  const canRetrySubmission = phase === 'error'
    && error?.code === 'request_outcome_unknown'
    && taskId === undefined
  const canPoll = Boolean(taskId && !busy)

  return useMemo(() => ({
    busy,
    canPoll,
    canRetrySubmission,
    error,
    idempotencyKey,
    phase,
    openTask,
    poll,
    requestLocked,
    response,
    startNew,
    submit,
    taskId,
  }), [
    busy,
    canPoll,
    canRetrySubmission,
    error,
    idempotencyKey,
    phase,
    openTask,
    poll,
    requestLocked,
    response,
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
