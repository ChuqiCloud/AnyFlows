import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'

type PlaygroundSettingsProps = {
  idPrefix: string
  modelPicker?: ReactNode
  systemPrompt: string
  temperatureEnabled: boolean
  temperature: number
  maxOutputTokensEnabled: boolean
  maxOutputTokens: number
  onSystemPromptChange: (value: string) => void
  onTemperatureEnabledChange: (value: boolean) => void
  onTemperatureChange: (value: number) => void
  onMaxOutputTokensEnabledChange: (value: boolean) => void
  onMaxOutputTokensChange: (value: number) => void
}

export function PlaygroundSettings(props: PlaygroundSettingsProps) {
  const { t } = useTranslation()
  const systemId = `${props.idPrefix}-system`
  const temperatureId = `${props.idPrefix}-temperature`
  const maxTokensId = `${props.idPrefix}-max-tokens`

  return (
    <div className="grid p-4">
      {props.modelPicker ? (
        <section className="grid gap-2.5 border-b border-dashed border-[var(--hairline)] py-4">
          <h2 className="text-xs font-semibold">{t('playground.settings.model')}</h2>
          {props.modelPicker}
        </section>
      ) : null}

      <section className="grid gap-2.5 border-b border-dashed border-[var(--hairline)] py-4">
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor={systemId}>
          <span>{t('playground.settings.systemPrompt')}</span>
          <Textarea
            id={systemId}
            className="min-h-24 resize-y text-xs leading-5"
            maxLength={16_384}
            value={props.systemPrompt}
            placeholder={t('playground.settings.systemPlaceholder')}
            onChange={(event) => props.onSystemPromptChange(event.target.value)}
          />
        </label>
      </section>

      <section className="grid gap-3 pt-4">
        <h2 className="text-xs font-semibold">{t('playground.settings.parameters')}</h2>
        <div className="grid gap-2">
          <div className="flex items-center justify-between gap-2">
            <label className="text-[0.6875rem] font-medium text-muted-foreground" htmlFor={temperatureId}>
              {t('playground.settings.temperature')}
            </label>
            <Switch id={temperatureId} checked={props.temperatureEnabled} onCheckedChange={props.onTemperatureEnabledChange} />
          </div>
          <div className="flex items-center gap-2">
            <input
              type="range"
              className="h-1.5 min-w-0 flex-1 cursor-pointer appearance-none rounded-pill bg-surface-2 accent-primary disabled:cursor-not-allowed disabled:opacity-40"
              min="0"
              max="2"
              step="0.1"
              disabled={!props.temperatureEnabled}
              value={props.temperature}
              aria-label={t('playground.settings.temperature')}
              onChange={(event) => props.onTemperatureChange(Number(event.target.value))}
            />
            <span className="w-8 text-right font-mono text-xs tabular-nums">{props.temperature.toFixed(1)}</span>
          </div>
        </div>
        <div className="grid gap-2">
          <div className="flex items-center justify-between gap-2">
            <label className="text-[0.6875rem] font-medium text-muted-foreground" htmlFor={maxTokensId}>
              {t('playground.settings.maxOutputTokens')}
            </label>
            <Switch id={maxTokensId} checked={props.maxOutputTokensEnabled} onCheckedChange={props.onMaxOutputTokensEnabledChange} />
          </div>
          <Input
            type="number"
            className="font-mono text-xs tabular-nums"
            min={1}
            max={1_000_000}
            disabled={!props.maxOutputTokensEnabled}
            value={props.maxOutputTokens}
            aria-label={t('playground.settings.maxOutputTokens')}
            onChange={(event) => props.onMaxOutputTokensChange(Number(event.target.value))}
          />
        </div>
      </section>
    </div>
  )
}
