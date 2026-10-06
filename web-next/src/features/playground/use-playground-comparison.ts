import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react'

import { streamPlaygroundChat } from './playground-client'
import {
  createPlaygroundSession,
  hasPlaygroundConversation,
  playgroundOverallState,
  playgroundRetryTurn,
  playgroundSessionReducer,
  type PlaygroundTurnSeed,
} from './playground-comparison-state'
import { classifyPlaygroundError } from './playground-error'
import { normalizePlaygroundModels } from './playground-model-selection'
import { resolvePlaygroundTargets, type PlaygroundProtocol } from './playground-protocol'
import type { PlaygroundMessage, PlaygroundSession, PlaygroundSharedSettings } from './playground-types'

function messageId() {
  return globalThis.crypto.randomUUID()
}

/** 管理多模型独立流；清空或卸载后，旧请求不得再写回当前比较结果。 */
export function usePlaygroundComparison(
  settings: PlaygroundSharedSettings,
  selectedModels: string[],
  protocolByModel: Readonly<Record<string, PlaygroundProtocol>>,
) {
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

  const restore = useCallback((restoredSessions: PlaygroundSession[]) => {
    requestSequenceRef.current += 1
    stopAll()
    controllersRef.current.clear()
    setDraft('')
    dispatch({ type: 'restore', sessions: restoredSessions })
  }, [stopAll])

  const runRequest = useCallback(async (input: {
    assistantId: string
    controller: AbortController
    model: string
    protocol: PlaygroundProtocol
    requestMessages: PlaygroundMessage[]
    sequence: number
  }) => {
    const { assistantId, controller, model, protocol, requestMessages, sequence } = input
    try {
      const result = await streamPlaygroundChat({
        ...settings,
        model,
        protocol,
        messages: requestMessages,
        signal: controller.signal,
        onText: (delta) => {
          if (requestSequenceRef.current !== sequence || controller.signal.aborted) return
          dispatch({ type: 'append', model, assistantId, delta })
        },
      })
      if (requestSequenceRef.current !== sequence) return
      if (controller.signal.aborted) {
        dispatch({ type: 'fail', model, assistantId, cancelled: true })
        return
      }
      if (result.interrupted) {
        dispatch({ type: 'interrupt', model, assistantId })
        return
      }
      dispatch({ type: 'complete', model, assistantId, usage: result.usage })
    } catch (error) {
      if (requestSequenceRef.current !== sequence) return
      dispatch({
        type: 'fail',
        model,
        assistantId,
        cancelled: controller.signal.aborted,
        errorKind: controller.signal.aborted ? undefined : classifyPlaygroundError(error),
      })
    } finally {
      if (controllersRef.current.get(model) === controller) {
        controllersRef.current.delete(model)
      }
    }
  }, [settings])

  const send = useCallback(async () => {
    const content = draft.trim()
    const targets = resolvePlaygroundTargets(models, protocolByModel)
    if (!content || !targets || controllersRef.current.size > 0) {
      return false
    }

    const sequence = ++requestSequenceRef.current
    const sharedUserId = messageId()
    const turns = targets.map(({ model, protocol }): PlaygroundTurnSeed & { requestMessages: PlaygroundMessage[]; protocol: PlaygroundProtocol } => {
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
        protocol,
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

    await Promise.allSettled(requests.map(({ turn, controller }) => runRequest({
      assistantId: turn.assistantMessage.id,
      controller,
      model: turn.model,
      protocol: turn.protocol,
      requestMessages: turn.requestMessages,
      sequence,
    })))
    return true
  }, [draft, models, protocolByModel, runRequest, sessions])

  const retry = useCallback(async (model: string) => {
    if (controllersRef.current.size > 0) return false
    const session = sessions.find((candidate) => candidate.model === model)
    const target = resolvePlaygroundTargets([model], protocolByModel)?.[0]
    if (!session || !target) return false
    const turn = playgroundRetryTurn(session)
    if (!turn) return false

    const sequence = ++requestSequenceRef.current
    const controller = new AbortController()
    controllersRef.current.set(model, controller)
    dispatch({ type: 'retry', model, assistantId: turn.assistantId })
    await runRequest({
      assistantId: turn.assistantId,
      controller,
      model,
      protocol: target.protocol,
      requestMessages: turn.requestMessages,
      sequence,
    })
    return true
  }, [protocolByModel, runRequest, sessions])

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
    retry,
    restore,
    send,
    sessions,
    setDraft,
    stopAll,
    stopModel,
    streaming: sessions.some((session) => session.requestState === 'streaming'),
  }
}
