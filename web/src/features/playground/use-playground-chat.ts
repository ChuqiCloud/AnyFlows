import { useCallback, useEffect, useRef, useState } from 'react'

import { streamPlaygroundChat } from './playground-client'
import { classifyPlaygroundError } from './playground-error'
import type {
  PlaygroundErrorKind,
  PlaygroundGenerationSettings,
  PlaygroundMessage,
  PlaygroundRequestState,
  PlaygroundUsage,
} from './playground-types'

function messageId() {
  return globalThis.crypto.randomUUID()
}

/** 管理单会话流式生命周期，并阻止已清空的旧请求回写新界面。 */
export function usePlaygroundChat(settings: PlaygroundGenerationSettings) {
  const [messages, setMessages] = useState<PlaygroundMessage[]>([])
  const [draft, setDraft] = useState('')
  const [requestState, setRequestState] = useState<PlaygroundRequestState>('idle')
  const [errorKind, setErrorKind] = useState<PlaygroundErrorKind>()
  const [usage, setUsage] = useState<PlaygroundUsage>()
  const controllerRef = useRef<AbortController | undefined>(undefined)
  const requestSequenceRef = useRef(0)

  const stop = useCallback(() => {
    controllerRef.current?.abort()
  }, [])

  const clear = useCallback(() => {
    requestSequenceRef.current += 1
    controllerRef.current?.abort()
    controllerRef.current = undefined
    setMessages([])
    setDraft('')
    setUsage(undefined)
    setErrorKind(undefined)
    setRequestState('idle')
  }, [])

  const send = useCallback(async () => {
    const content = draft.trim()
    if (!content || controllerRef.current) return false

    const sequence = ++requestSequenceRef.current
    const controller = new AbortController()
    const userMessage: PlaygroundMessage = {
      id: messageId(),
      role: 'user',
      content,
      status: 'complete',
    }
    const assistantId = messageId()
    const requestMessages = [...messages, userMessage]

    controllerRef.current = controller
    setDraft('')
    setErrorKind(undefined)
    setUsage(undefined)
    setRequestState('streaming')
    setMessages([
      ...requestMessages,
      { id: assistantId, role: 'assistant', content: '', status: 'streaming' },
    ])

    try {
      const result = await streamPlaygroundChat({
        ...settings,
        messages: requestMessages,
        signal: controller.signal,
        onText: (delta) => {
          if (requestSequenceRef.current !== sequence) return
          setMessages((current) => current.map((message) => (
            message.id === assistantId
              ? { ...message, content: message.content + delta }
              : message
          )))
        },
      })
      if (requestSequenceRef.current !== sequence) return true

      setMessages((current) => current.map((message) => (
        message.id === assistantId ? { ...message, status: 'complete' } : message
      )))
      setUsage(result.usage)
      setRequestState('idle')
      return true
    } catch (error) {
      if (requestSequenceRef.current !== sequence) return false

      const cancelled = controller.signal.aborted
      setMessages((current) => current.map((message) => (
        message.id === assistantId
          ? { ...message, status: cancelled ? 'cancelled' : 'error' }
          : message
      )))
      setErrorKind(cancelled ? undefined : classifyPlaygroundError(error))
      setRequestState(cancelled ? 'cancelled' : 'error')
      return false
    } finally {
      if (requestSequenceRef.current === sequence) controllerRef.current = undefined
    }
  }, [draft, messages, settings])

  useEffect(() => () => {
    requestSequenceRef.current += 1
    controllerRef.current?.abort()
  }, [])

  return {
    clear,
    draft,
    errorKind,
    messages,
    requestState,
    send,
    setDraft,
    stop,
    usage,
  }
}
