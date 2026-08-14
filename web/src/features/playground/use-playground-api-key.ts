import { useCallback, useEffect, useState } from 'react'

import { apiClient } from '@/lib/api'
import { createPlaygroundToken } from '@/lib/api/generated/sdk.gen'
import { getPlaygroundApiKey, setPlaygroundApiKey } from './playground-api-key'

export type PlaygroundApiKeyProvisioning = 'idle' | 'pending' | 'failed'

let playgroundKeyProvisioning: Promise<string> | undefined

/** 在当前标签页复用同一次签发请求，避免开发模式重复挂载创建多把 Key。 */
function provisionPlaygroundApiKey() {
  const existing = getPlaygroundApiKey()
  if (existing) return Promise.resolve(existing)
  playgroundKeyProvisioning ??= createPlaygroundToken({ client: apiClient }).then(({ data }) => {
    setPlaygroundApiKey(data.api_key)
    return data.api_key
  }).finally(() => {
    playgroundKeyProvisioning = undefined
  })
  return playgroundKeyProvisioning
}

/** 管理登录会话自动签发的试炼场 Key，不向产品界面暴露手动写入入口。 */
export function usePlaygroundApiKey() {
  const [apiKey, setApiKey] = useState(() => getPlaygroundApiKey() ?? '')
  const [provisioning, setProvisioning] = useState<PlaygroundApiKeyProvisioning>('idle')

  const provision = useCallback(() => {
    setProvisioning('pending')
    void provisionPlaygroundApiKey()
      .then((issuedApiKey) => {
        setApiKey(issuedApiKey)
        setProvisioning('idle')
      })
      .catch(() => {
        setProvisioning('failed')
      })
  }, [])

  useEffect(() => {
    if (!getPlaygroundApiKey()) provision()
  }, [provision])

  /** 从当前标签页存储重新载入托管 Key，用于不同试炼模式切换时同步状态。 */
  const reload = useCallback(() => {
    const current = getPlaygroundApiKey() ?? ''
    setApiKey(current)
    if (current) setProvisioning('idle')
  }, [])

  return { apiKey, provisioning, provision, reload }
}
