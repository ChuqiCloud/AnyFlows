import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react'

import { streamPlaygroundChat } from './playground-client'
import {
  createPlaygroundSession,
  hasPlaygroundConversation,
  playgroundOverallState,
  playgroundSessionReducer,
  type PlaygroundTurnSeed,
} from './playground-comparison-state'
import { classifyPlaygroundError } from './playground-error'
import { normalizePlaygroundModels } from './playground-model-selection'
import type { PlaygroundMessage, PlaygroundSharedSettings } from './playground-types'

function messageId() {
  return globalThis.crypto.randomUUID()
}

/** 管理多模型独立流；清空或卸载后，旧请求不得再写回当前比较结果。 */
export function usePlaygroundComparison(settings: PlaygroundSharedSettings, selectedModels: string[]) {
  const models = useMemo(
    () => normalizePlaygroundModels(selectedModels),
    [selectedModels],
  )
  const [sessions, dispatch] = useReducer(
    playgroundSessionReducer,
    models.map(createPlaygroundSession),
  )
  const [draft, setDraft] = useState('')
  const controllersRef = useRef(new Map<string, AbortController>())
  const requestSequenceRef = useRef(0)

  useEffect(() => {
    dispatch({ type: 'sync', models })
  }, [models])

  const stopModel = useCallback((model: string) => {
    controllersRef.current.get(model)?.abort()
  }, [])

  const stopAll = useCallback(() => {
    for (const controller of controllersRef.current.values()) controller.abort()
  }, [])

  const clear = useCallback(() => {
    requestSequenceRef.current += 1
    stopAll()
    controllersRef.current.clear()
    setDraft('')
    dispatch({ type: 'clear', models })
  }, [models, stopAll])

  const send = useCallback(async () => {
    const content = draft.trim()
    if (!content || !settings.apiKey || models.length === 0 || controllersRef.current.size > 0) {
      return false
    }

    const sequence = ++requestSequenceRef.current
    const sharedUserId = messageId()
    const turns = models.map((model): PlaygroundTurnSeed & { requestMessages: PlaygroundMessage[] } => {
      const current = sessions.find((session) => session.model === model)
        ?? createPlaygroundSession(model)
      const userMessage: PlaygroundMessage = {
        id: sharedUserId,
        role: 'user',
        content,
        status: 'complete',
      }
      const assistantMessage: PlaygroundMessage = {
        id: messageId(),
        role: 'assistant',
        content: '',
        status: 'streaming',
      }
      return {
        model,
        userMessage,
        assistantMessage,
        requestMessages: [...current.messages, userMessage],
      }
    })

    const requests = turns.map((turn) => {
      const controller = new AbortController()
      controllersRef.current.set(turn.model, controller)
      return { turn, controller }
    })
    setDraft('')
    dispatch({ type: 'begin', turns })

    await Promise.allSettled(requests.map(async ({ turn, controller }) => {
      try {
        const result = await streamPlaygroundChat({
          ...settings,
          model: turn.model,
          messages: turn.requestMessages,
          signal: controller.signal,
          onText: (delta) => {
            if (requestSequenceRef.current !== sequence || controller.signal.aborted) return
            dispatch({
              type: 'append',
              model: turn.model,
              assistantId: turn.assistantMessage.id,
              delta,
            })
          },
        })
        if (requestSequenceRef.current !== sequence) return
        if (controller.signal.aborted) {
          dispatch({
            type: 'fail',
            model: turn.model,
            assistantId: turn.assistantMessage.id,
            cancelled: true,
          })
          return
        }
        dispatch({
          type: 'complete',
          model: turn.model,
          assistantId: turn.assistantMessage.id,
          usage: result.usage,
        })
      } catch (error) {
        if (requestSequenceRef.current !== sequence) return
        dispatch({
          type: 'fail',
          model: turn.model,
          assistantId: turn.assistantMessage.id,
          cancelled: controller.signal.aborted,
          errorKind: controller.signal.aborted ? undefined : classifyPlaygroundError(error),
        })
      } finally {
        if (controllersRef.current.get(turn.model) === controller) {
          controllersRef.current.delete(turn.model)
        }
      }
    }))
    return true
  }, [draft, models, sessions, settings])

  useEffect(() => () => {
    requestSequenceRef.current += 1
    stopAll()
    controllersRef.current.clear()
  }, [stopAll])

  return {
    clear,
    draft,
    hasConversation: hasPlaygroundConversation(sessions),
    overallState: playgroundOverallState(sessions),
    send,
    sessions,
    setDraft,
    stopAll,
    stopModel,
    streaming: sessions.some((session) => session.requestState === 'streaming'),
  }
}
