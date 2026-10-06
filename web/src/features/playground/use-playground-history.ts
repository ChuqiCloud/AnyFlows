import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'

import type { PlaygroundConversationResponse } from './playground-history-api'
import { playgroundHistoryQueryKey, savePlaygroundConversation } from './playground-history-api'
import {
  createPlaygroundConversationId,
  playgroundHistoryFingerprint,
} from './playground-history-snapshot'
import { buildPlaygroundShareSnapshot } from './playground-share-snapshot'
import type { PlaygroundSession } from './playground-types'

export type PlaygroundHistorySaveState =
  | 'idle'
  | 'saving'
  | 'saved'
  | 'error'
  | 'conflict'
  | 'limit'

type Binding = { conversationId: string; revision?: number }

function errorCode(error: unknown) {
  if (typeof error !== 'object' || error === null || !('details' in error)) return undefined
  const details = (error as { details?: unknown }).details
  return typeof details === 'object' && details !== null && 'code' in details
    ? (details as { code?: unknown }).code
    : undefined
}

function classifySaveError(error: unknown): PlaygroundHistorySaveState {
  const code = errorCode(error)
  if (code === 'playground_conversation_conflict') return 'conflict'
  if (code === 'playground_conversation_limit_reached') return 'limit'
  return 'error'
}

/** 在完整终态后串行自动保存，并用 generation 隔离清空或恢复前的旧响应。 */
export function usePlaygroundHistory(sessions: PlaygroundSession[], streaming: boolean) {
  const queryClient = useQueryClient()
  const snapshot = useMemo(() => buildPlaygroundShareSnapshot(sessions), [sessions])
  const fingerprint = snapshot.ok ? playgroundHistoryFingerprint(snapshot.sessions) : undefined
  const bindingRef = useRef<Binding>({ conversationId: createPlaygroundConversationId() })
  const generationRef = useRef(0)
  const lastAttemptedRef = useRef<string | undefined>(undefined)
  const [binding, setBinding] = useState(bindingRef.current)
  const [saveState, setSaveState] = useState<PlaygroundHistorySaveState>('idle')
  const [retrySequence, setRetrySequence] = useState(0)

  useEffect(() => {
    if (streaming || !snapshot.ok || !fingerprint || lastAttemptedRef.current === fingerprint) {
      return
    }
    const generation = generationRef.current
    const currentBinding = bindingRef.current
    lastAttemptedRef.current = fingerprint
    setSaveState('saving')
    void savePlaygroundConversation({
      conversationId: currentBinding.conversationId,
      revision: currentBinding.revision,
      sessions: snapshot.sessions,
    }).then((conversation) => {
      if (generationRef.current !== generation) return
      const nextBinding = {
        conversationId: conversation.conversation_id,
        revision: conversation.revision,
      }
      bindingRef.current = nextBinding
      setBinding(nextBinding)
      setSaveState('saved')
      void queryClient.invalidateQueries({ queryKey: playgroundHistoryQueryKey })
    }).catch((error: unknown) => {
      if (generationRef.current === generation) setSaveState(classifySaveError(error))
    })
  }, [fingerprint, queryClient, retrySequence, snapshot, streaming])

  const startNewConversation = useCallback(() => {
    generationRef.current += 1
    const nextBinding = { conversationId: createPlaygroundConversationId() }
    bindingRef.current = nextBinding
    lastAttemptedRef.current = undefined
    setBinding(nextBinding)
    setSaveState('idle')
  }, [])

  const bindRestoredConversation = useCallback((conversation: PlaygroundConversationResponse) => {
    generationRef.current += 1
    const nextBinding = {
      conversationId: conversation.conversation_id,
      revision: conversation.revision,
    }
    bindingRef.current = nextBinding
    lastAttemptedRef.current = playgroundHistoryFingerprint(conversation.sessions)
    setBinding(nextBinding)
    setSaveState('saved')
  }, [])

  const retry = useCallback(() => {
    lastAttemptedRef.current = undefined
    setRetrySequence((current) => current + 1)
  }, [])

  return {
    activeConversationId: binding.revision === undefined ? undefined : binding.conversationId,
    bindRestoredConversation,
    retry,
    saveState,
    saving: saveState === 'saving',
    startNewConversation,
  }
}
