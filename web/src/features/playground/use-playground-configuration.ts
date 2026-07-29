import { useCallback, useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useModelCatalog } from '@/features/models/model-api'
import {
  clearPlaygroundApiKey,
  getPlaygroundApiKey,
  isPlaygroundApiKey,
  setPlaygroundApiKey,
} from './playground-api-key'
import { normalizePlaygroundModels } from './playground-model-selection'

/** 收拢当前标签页凭据、模型目录与共享生成参数。 */
export function usePlaygroundConfiguration() {
  const { t } = useTranslation()
  const [apiKey, setApiKey] = useState(() => getPlaygroundApiKey() ?? '')
  const [apiKeyDraft, setApiKeyDraft] = useState(apiKey)
  const [apiKeyError, setApiKeyError] = useState<string>()
  const [selectedModels, setSelectedModels] = useState<string[]>([])
  const [modelSearchDraft, setModelSearchDraft] = useState('')
  const [modelSearch, setModelSearch] = useState('')
  const [systemPrompt, setSystemPrompt] = useState('')
  const [temperatureEnabled, setTemperatureEnabled] = useState(false)
  const [temperature, setTemperature] = useState(0.7)
  const [maxOutputTokensEnabled, setMaxOutputTokensEnabled] = useState(false)
  const [maxOutputTokens, setMaxOutputTokens] = useState(4096)

  const catalogQuery = useModelCatalog(modelSearch, 'all', true)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const catalogInvalid = catalogQuery.isError
    || (pricingScope !== undefined && pricingScope !== 'group')

  useEffect(() => {
    const timer = window.setTimeout(() => setModelSearch(modelSearchDraft.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [modelSearchDraft])

  useEffect(() => {
    if (selectedModels.length === 0 && models[0] && !catalogInvalid) {
      setSelectedModels([models[0].model])
    }
  }, [catalogInvalid, models, selectedModels.length])

  const saveApiKey = useCallback(() => {
    if (!isPlaygroundApiKey(apiKeyDraft)) {
      setApiKeyError(t('playground.apiKey.invalid'))
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

  const updateApiKeyDraft = useCallback((value: string) => {
    setApiKeyDraft(value)
    setApiKeyError(undefined)
  }, [])

  const updateSelectedModels = useCallback((modelsToSelect: string[]) => {
    setSelectedModels(normalizePlaygroundModels(modelsToSelect))
  }, [])

  const sharedSettings = useMemo(() => ({
    apiKey,
    systemPrompt,
    temperature: temperatureEnabled ? temperature : undefined,
    maxOutputTokens: maxOutputTokensEnabled ? maxOutputTokens : undefined,
  }), [apiKey, maxOutputTokens, maxOutputTokensEnabled, systemPrompt, temperature, temperatureEnabled])

  return {
    apiKey,
    apiKeyDraft,
    apiKeyError,
    catalogInvalid,
    catalogQuery,
    clearApiKey,
    maxOutputTokens,
    maxOutputTokensEnabled,
    modelSearchDraft,
    models,
    saveApiKey,
    selectedModels,
    setMaxOutputTokens: (value: number) => setMaxOutputTokens(
      Math.min(1_000_000, Math.max(1, value || 1)),
    ),
    setMaxOutputTokensEnabled,
    setModelSearchDraft,
    setSelectedModels: updateSelectedModels,
    setSystemPrompt,
    setTemperature: (value: number) => setTemperature(Math.min(2, Math.max(0, value))),
    setTemperatureEnabled,
    sharedSettings,
    systemPrompt,
    temperature,
    temperatureEnabled,
    updateApiKeyDraft,
  }
}
