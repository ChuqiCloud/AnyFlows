import { Input, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'

type JsonObject = Record<string, unknown>

type FieldSchema = {
  type?: unknown
  title?: unknown
  description?: unknown
  format?: unknown
  enum?: unknown
  minimum?: unknown
  maximum?: unknown
  minLength?: unknown
  maxLength?: unknown
}

const sensitiveFieldPattern = /apikey|token|secret|password|credential|cookie|authorization/i

function objectValue(value: unknown): JsonObject | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined
  return value as JsonObject
}

function localizedText(value: unknown, locale: string): string | undefined {
  if (typeof value === 'string') return value
  const localized = objectValue(value)
  if (!localized) return undefined
  const language = locale.toLowerCase().startsWith('zh') ? 'zh' : 'en'
  const text = localized[language] ?? localized.en ?? Object.values(localized)[0]
  return typeof text === 'string' ? text : undefined
}

function parseObject(value: string): JsonObject {
  try {
    return objectValue(JSON.parse(value)) ?? {}
  } catch {
    return {}
  }
}

function updateValue(value: string, key: string, nextValue: unknown) {
  const next = { ...parseObject(value) }
  if (nextValue === undefined) delete next[key]
  else next[key] = nextValue
  return JSON.stringify(next, null, 2)
}

function fieldType(schema: FieldSchema): 'string' | 'number' | 'integer' | 'boolean' | undefined {
  if (schema.type === 'string' || schema.type === 'number' || schema.type === 'integer' || schema.type === 'boolean') {
    return schema.type
  }
  if (Array.isArray(schema.type) && schema.type.includes('null')) {
    const valueTypes = schema.type.filter((item) => item !== 'null')
    if (valueTypes.length === 1 && (valueTypes[0] === 'string' || valueTypes[0] === 'number' || valueTypes[0] === 'integer')) {
      return valueTypes[0]
    }
  }
  return undefined
}

function validEnum(value: unknown): value is (string | number | boolean)[] {
  return Array.isArray(value)
    && value.length > 0
    && value.length <= 100
    && value.every((item) => typeof item === 'string' || typeof item === 'number' || typeof item === 'boolean')
}

/**
 * Render only host-owned controls from the plugin's validated JSON Schema.
 * Plugin packages cannot supply components, markup, styles, routes or URLs.
 */
export function PluginConfigEditor({ schema, value, onChange, locale }: {
  schema: unknown
  value: string
  onChange: (value: string) => void
  locale: string
}) {
  const { t } = useTranslation()
  const root = objectValue(schema)
  const properties = objectValue(root?.properties)
  const required = Array.isArray(root?.required) ? new Set(root.required.filter((item): item is string => typeof item === 'string')) : new Set<string>()
  const safeFields = Object.entries(properties ?? {}).flatMap(([key, rawSchema]) => {
    if (sensitiveFieldPattern.test(key.replace(/[^a-z0-9]/gi, ''))) return []
    const field = objectValue(rawSchema) as FieldSchema | undefined
    if (!field || !fieldType(field) || field.format === 'password') return []
    if (field.enum !== undefined && !validEnum(field.enum)) return []
    return [{ key, field, type: fieldType(field)!, required: required.has(key) }]
  }).slice(0, 64)
  const current = parseObject(value)

  const control = (key: string, field: FieldSchema, type: 'string' | 'number' | 'integer' | 'boolean', requiredField: boolean) => {
    const label = localizedText(field.title, locale) ?? key
    const description = localizedText(field.description, locale)
    const present = Object.hasOwn(current, key)
    const currentValue = current[key]
    if (type === 'boolean') {
      return <label key={key} className="flex items-center gap-2 rounded-lg border border-[var(--hairline)] p-3 text-sm">
        <input type="checkbox" checked={currentValue === true} onChange={(event) => onChange(updateValue(value, key, event.target.checked))} />
        <span><strong>{label}</strong>{requiredField ? <span className="ml-1 text-danger">*</span> : null}{description ? <small className="mt-1 block text-muted-foreground">{description}</small> : null}</span>
      </label>
    }
    if (validEnum(field.enum)) {
      return <label key={key} className="grid gap-1 text-xs font-medium">{label}{requiredField ? <span className="text-danger">*</span> : null}
        <select className="h-10 w-full rounded-lg border border-[var(--hairline)] bg-background px-3 text-sm" value={present ? JSON.stringify(currentValue) : ''} onChange={(event) => {
          if (!event.target.value) { onChange(updateValue(value, key, undefined)); return }
          try { onChange(updateValue(value, key, JSON.parse(event.target.value))) } catch { onChange(updateValue(value, key, event.target.value)) }
        }}>
          <option value="">—</option>
          {field.enum.map((option) => <option key={JSON.stringify(option)} value={JSON.stringify(option)}>{String(option)}</option>)}
        </select>
        {description ? <small className="font-normal text-muted-foreground">{description}</small> : null}
      </label>
    }
    const inputType = type === 'number' || type === 'integer' ? 'number' : 'text'
    const onTextChange = (next: string) => {
      if (next === '') { onChange(updateValue(value, key, undefined)); return }
      if (type === 'number' || type === 'integer') {
        const parsed = Number(next)
        if (!Number.isFinite(parsed) || (type === 'integer' && !Number.isInteger(parsed))) return
        onChange(updateValue(value, key, parsed))
      } else onChange(updateValue(value, key, next))
    }
    const textValue = typeof currentValue === 'string' || typeof currentValue === 'number' ? String(currentValue) : ''
    if (type === 'string' && field.format === 'textarea') {
      return <Textarea key={key} label={label} description={description} isRequired={requiredField} maxLength={typeof field.maxLength === 'number' ? field.maxLength : undefined} minRows={3} value={textValue} onValueChange={onTextChange} />
    }
    return <Input key={key} label={label} description={description} size="sm" type={inputType} isRequired={requiredField} min={typeof field.minimum === 'number' ? field.minimum : undefined} max={typeof field.maximum === 'number' ? field.maximum : undefined} step={type === 'integer' ? 1 : undefined} minLength={typeof field.minLength === 'number' ? field.minLength : undefined} maxLength={typeof field.maxLength === 'number' ? field.maxLength : undefined} value={textValue} onValueChange={onTextChange} />
  }

  return <div className="grid gap-3">
    {safeFields.map(({ key, field, type, required: isRequired }) => control(key, field, type, isRequired))}
    <details className="text-xs">
      <summary className="cursor-pointer font-medium">{t('pluginLifecycle.configEditor.advanced')}</summary>
      <Textarea aria-label={t('pluginLifecycle.fields.config')} description={t('pluginLifecycle.configEditor.hint')} minRows={5} value={value} onValueChange={onChange} classNames={{ input: 'font-mono text-xs' }} />
    </details>
  </div>
}
