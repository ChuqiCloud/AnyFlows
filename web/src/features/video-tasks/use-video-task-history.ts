import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { listVideoTasks, videoTaskError } from './video-task-client'
import type { VideoTaskApiError, VideoTaskListItem } from './video-task-types'

type HistoryPhase = 'idle' | 'loading' | 'ready' | 'loadingMore' | 'error'

/** 按当前标签页 API Key 隔离视频任务历史，凭据变化时立即清空旧 owner 数据。 */
export function useVideoTaskHistory(apiKey: string) {
  const [items, setItems] = useState<VideoTaskListItem[]>([])
  const [nextCursor, setNextCursor] = useState<string>()
  const [phase, setPhase] = useState<HistoryPhase>('idle')
  const [error, setError] = useState<VideoTaskApiError>()
  const controllerRef = useRef<AbortController | null>(null)

  const requestPage = useCallback(async (before?: string) => {
    if (!apiKey) return
    controllerRef.current?.abort()
    const controller = new AbortController()
    controllerRef.current = controller
    setPhase(before ? 'loadingMore' : 'loading')
    setError(undefined)
    try {
      const page = await listVideoTasks(apiKey, before, controller.signal)
      setItems((current) => before ? mergeById(current, page.data) : page.data)
      setNextCursor(page.next_cursor)
      setPhase('ready')
    } catch (cause) {
      const detail = videoTaskError(cause)
      if (detail.code === 'cancelled') return
      setError(detail)
      setPhase('error')
    }
  }, [apiKey])

  useEffect(() => {
    controllerRef.current?.abort()
    setItems([])
    setNextCursor(undefined)
    setError(undefined)
    if (!apiKey) {
      setPhase('idle')
      return
    }
    void requestPage()
  }, [apiKey, requestPage])

  useEffect(() => () => controllerRef.current?.abort(), [])

  const refresh = useCallback(() => {
    void requestPage()
  }, [requestPage])

  const loadMore = useCallback(() => {
    if (nextCursor && phase === 'ready') void requestPage(nextCursor)
  }, [nextCursor, phase, requestPage])

  return useMemo(() => ({
    error,
    items,
    loadMore,
    nextCursor,
    phase,
    refresh,
  }), [error, items, loadMore, nextCursor, phase, refresh])
}

function mergeById(current: VideoTaskListItem[], incoming: VideoTaskListItem[]) {
  const known = new Set(current.map((item) => item.id))
  return [...current, ...incoming.filter((item) => !known.has(item.id))]
}
