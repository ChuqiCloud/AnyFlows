import { useCallback, useEffect, useMemo, useState } from 'react'

import { EMPTY_MODEL_CATALOG_FILTERS, useModelCatalog } from '@/features/models/model-api'
import {
  DEFAULT_PLAYGROUND_COMPARISON_ENABLED,
  modelsForPlaygroundSelectionMode,
  normalizePlaygroundModels,
} from './playground-model-selection'
import { playgroundProtocolForModel, type PlaygroundProtocol } from './playground-protocol'

const DEFAULT_TEMPERATURE = 0.7
const DEFAULT_MAX_OUTPUT_TOKENS = 8192

/** 收拢当前标签页凭据、模型目录与共享生成参数。 */
export function usePlaygroundConfiguration() {
  const [selectedModels, setSelectedModels] = useState<string[]>([])
  const [comparisonEnabled, setComparisonEnabled] = useState(
    DEFAULT_PLAYGROUND_COMPARISON_ENABLED,
  )
  const [protocolByModel, setProtocolByModel] = useState<Record<string, PlaygroundProtocol>>({})
  const [modelSearchDraft, setModelSearchDraft] = useState('')
  const [modelSearch, setModelSearch] = useState('')
  const [systemPrompt, setSystemPrompt] = useState('')
  const [temperatureEnabled, setTemperatureEnabled] = useState(false)
  const [temperature, setTemperature] = useState(DEFAULT_TEMPERATURE)
  const [maxOutputTokensEnabled, setMaxOutputTokensEnabled] = useState(false)
  const [maxOutputTokens, setMaxOutputTokens] = useState(DEFAULT_MAX_OUTPUT_TOKENS)

  const catalogQuery = useModelCatalog(modelSearch, EMPTY_MODEL_CATALOG_FILTERS, true)
  const models = useMemo(
    () => catalogQuery.data?.pages.flatMap((page) => page.models) ?? [],
    [catalogQuery.data],
  )
  const routableModels = useMemo(
    () => models.filter((item) => playgroundProtocolForModel(item) !== undefined),
    [models],
  )
  useEffect(() => {
    setProtocolByModel((current) => {
      const next = { ...current }
      let changed = false
      for (const item of models) {
        const protocol = playgroundProtocolForModel(item)
        if (protocol === undefined) {
          if (Object.hasOwn(next, item.model)) {
            delete next[item.model]
            changed = true
          }
        } else if (next[item.model] !== protocol) {
          next[item.model] = protocol
          changed = true
        }
      }
      return changed ? next : current
    })
  }, [models])
  const pricingScope = catalogQuery.data?.pages[0]?.pricing_scope
  const catalogInvalid = catalogQuery.isError
    || (pricingScope !== undefined && pricingScope !== 'group')

  useEffect(() => {
    const timer = window.setTimeout(() => setModelSearch(modelSearchDraft.trim()), 250)
    return () => window.clearTimeout(timer)
  }, [modelSearchDraft])

  useEffect(() => {
    if (selectedModels.length === 0 && routableModels[0] && !catalogInvalid) {
      setSelectedModels([routableModels[0].model])
    }
  }, [catalogInvalid, routableModels, selectedModels.length])

  const updateSelectedModels = useCallback((
    modelsToSelect: string[],
    comparisonEnabledToApply: boolean,
  ) => {
    setComparisonEnabled(comparisonEnabledToApply)
    setSelectedModels(modelsForPlaygroundSelectionMode(
      modelsToSelect,
      comparisonEnabledToApply,
    ))
  }, [])

  /** 恢复历史时只恢复模型，显式清空未持久化的生成设置。 */
  const restoreConversationConfiguration = useCallback((modelsToSelect: string[]) => {
    const normalizedModels = normalizePlaygroundModels(modelsToSelect)
    const restoredComparisonEnabled = normalizedModels.length > 1
    setComparisonEnabled(restoredComparisonEnabled)
    setSelectedModels(modelsForPlaygroundSelectionMode(
      normalizedModels,
      restoredComparisonEnabled,
    ))
    setSystemPrompt('')
    setTemperatureEnabled(false)
    setTemperature(DEFAULT_TEMPERATURE)
    setMaxOutputTokensEnabled(false)
    setMaxOutputTokens(DEFAULT_MAX_OUTPUT_TOKENS)
  }, [])

  const sharedSettings = useMemo(() => ({
    systemPrompt,
    temperature: temperatureEnabled ? temperature : undefined,
    maxOutputTokens: maxOutputTokensEnabled ? maxOutputTokens : undefined,
  }), [maxOutputTokens, maxOutputTokensEnabled, systemPrompt, temperature, temperatureEnabled])

  const selectedProtocolsReady = selectedModels.every((model) => protocolByModel[model] !== undefined)

  return {
    catalogInvalid,
    catalogQuery,
    comparisonEnabled,
    maxOutputTokens,
    maxOutputTokensEnabled,
    modelSearchDraft,
    models,
    protocolByModel,
    restoreConversationConfiguration,
    routableModels,
    selectedModels,
    selectedProtocolsReady,
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
  }
}
