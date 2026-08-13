import type { FormEvent, ReactNode } from 'react'
import { Eye, EyeOff, KeyRound, Save, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'

type PlaygroundSettingsProps = {
  idPrefix: string
  modelPicker?: ReactNode
  apiKeyDraft: string
  apiKeySaved: boolean
  apiKeyError?: string
  systemPrompt: string
  temperatureEnabled: boolean
  temperature: number
  maxOutputTokensEnabled: boolean
  maxOutputTokens: number
  onApiKeyDraftChange: (value: string) => void
  onSaveApiKey: () => void
  onClearApiKey: () => void
  onSystemPromptChange: (value: string) => void
  onTemperatureEnabledChange: (value: boolean) => void
  onTemperatureChange: (value: number) => void
  onMaxOutputTokensEnabledChange: (value: boolean) => void
  onMaxOutputTokensChange: (value: number) => void
}

export function PlaygroundSettings(props: PlaygroundSettingsProps) {
  const { t } = useTranslation()
  const [showApiKey, setShowApiKey] = useState(false)
  const keyId = `${props.idPrefix}-api-key`
  const systemId = `${props.idPrefix}-system`
  const temperatureId = `${props.idPrefix}-temperature`
  const maxTokensId = `${props.idPrefix}-max-tokens`

  const saveKey = (event: FormEvent) => {
    event.preventDefault()
    props.onSaveApiKey()
  }

  return (
    <div className="grid p-4">
      <section className="grid gap-2.5 border-b border-dashed border-[var(--hairline)] pb-4">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-xs font-semibold">{t('playground.settings.access')}</h2>
          <Badge className={props.apiKeySaved ? 'bg-success/12 text-success' : 'bg-warning/12 text-warning'}>
            {t(props.apiKeySaved ? 'playground.apiKey.ready' : 'playground.apiKey.missing')}
          </Badge>
        </div>
        <form className="grid gap-2" onSubmit={saveKey}>
          <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor={keyId}>
            <span>{t('playground.apiKey.label')}</span>
            <span className="relative block">
              <KeyRound className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2" aria-hidden="true" />
              <Input
                id={keyId}
                type={showApiKey ? 'text' : 'password'}
                className="px-8 font-mono text-xs"
                value={props.apiKeyDraft}
                autoComplete="off"
                autoCapitalize="none"
                maxLength={512}
                spellCheck={false}
                aria-invalid={props.apiKeyError !== undefined}
                placeholder="sk-af-..."
                onChange={(event) => props.onApiKeyDraftChange(event.target.value)}
              />
              <button
                type="button"
                className="absolute right-1 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
                aria-label={t(showApiKey ? 'playground.apiKey.hide' : 'playground.apiKey.show')}
                onClick={() => setShowApiKey((current) => !current)}
              >
                {showApiKey ? <EyeOff className="size-3.5" aria-hidden="true" /> : <Eye className="size-3.5" aria-hidden="true" />}
              </button>
            </span>
          </label>
          {props.apiKeyError ? <p role="alert" className="text-xs text-destructive">{props.apiKeyError}</p> : null}
          <div className="grid grid-cols-2 gap-2">
            <Button type="submit" size="sm" variant="secondary">
              <Save aria-hidden="true" />{t('playground.apiKey.save')}
            </Button>
            <Button type="button" size="sm" variant="ghost" disabled={!props.apiKeyDraft && !props.apiKeySaved} onClick={props.onClearApiKey}>
              <Trash2 aria-hidden="true" />{t('playground.apiKey.clear')}
            </Button>
          </div>
        </form>
      </section>

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
