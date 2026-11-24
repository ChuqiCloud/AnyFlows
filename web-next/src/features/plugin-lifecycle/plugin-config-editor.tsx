import { Button, Input, Textarea } from '@heroui/react'
import { Plus, Trash2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

type JsonObject = Record<string, unknown>
type JsonPath = Array<string | number>
type FieldSchema = JsonObject & {
  type?: unknown
  title?: unknown
  description?: unknown
  format?: unknown
  enum?: unknown
  properties?: unknown
  required?: unknown
  items?: unknown
  minimum?: unknown
  maximum?: unknown
  minLength?: unknown
  maxLength?: unknown
  minItems?: unknown
  maxItems?: unknown
  default?: unknown
  'x-ui-group'?: unknown
  'x-ui-order'?: unknown
  'x-ui-placeholder'?: unknown
  'x-ui-label'?: unknown
  'x-ui-description'?: unknown
  'x-ui-visible-when'?: unknown
  'x-ui-enum-labels'?: unknown
}

const sensitiveFieldPattern = /password|secret|credential|authorization|apikey|accesstoken|refreshtoken|idtoken|privatekey|cookie|bearer|token$/i
const MAX_RENDER_DEPTH = 6
const MAX_RENDERED_FIELDS = 128
const MAX_ARRAY_ITEMS = 64

function objectValue(value: unknown): JsonObject | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined
  return value as JsonObject
}

function safeConfigKey(key: string) {
  return key !== '__proto__' && key !== 'prototype' && key !== 'constructor'
}

function localizedText(value: unknown, locale: string): string | undefined {
  if (typeof value === 'string') {
    return value.length <= 512 && !/[\u0000-\u001f\u007f<>]/u.test(value) ? value : undefined
  }
  const localized = objectValue(value)
  if (!localized || Object.keys(localized).some((key) => key !== 'en' && key !== 'zh')) return undefined
  const preferred = locale.toLowerCase().startsWith('zh') ? localized.zh : localized.en
  const text = preferred ?? localized.en ?? localized.zh
  return typeof text === 'string' && text.length <= 512 && !/[\u0000-\u001f\u007f<>]/u.test(text) ? text : undefined
}

function parseJson(value: string): { valid: true; value: unknown } | { valid: false } {
  try {
    return { valid: true, value: JSON.parse(value) as unknown }
  } catch {
    return { valid: false }
  }
}

function readPath(value: unknown, path: JsonPath): unknown {
  let current = value
  for (const part of path) {
    if (typeof part === 'number') {
      if (!Array.isArray(current)) return undefined
      current = current[part]
    } else {
      const object = objectValue(current)
      if (!object || !Object.hasOwn(object, part)) return undefined
      current = object[part]
    }
  }
  return current
}

function writePath(value: unknown, path: JsonPath, nextValue: unknown): unknown {
  const write = (current: unknown, depth: number): unknown => {
    const part = path[depth]
    const isArray = typeof part === 'number'
    const output: JsonObject | unknown[] = isArray
      ? Array.isArray(current) ? [...current] : []
      : { ...(objectValue(current) ?? {}) }
    if (depth === path.length - 1) {
      if (isArray) (output as unknown[])[part as number] = nextValue ?? null
      else if (nextValue === undefined) delete (output as JsonObject)[part as string]
      else Object.defineProperty(output, part as string, { value: nextValue, enumerable: true, configurable: true, writable: true })
      return output
    }
    const child = isArray
      ? (output as unknown[])[part as number]
      : (output as JsonObject)[part as string]
    const updated = write(child, depth + 1)
    if (isArray) (output as unknown[])[part as number] = updated
    else Object.defineProperty(output, part as string, { value: updated, enumerable: true, configurable: true, writable: true })
    return output
  }

  return write(value, 0)
}

function schemaType(schema: FieldSchema): 'string' | 'number' | 'integer' | 'boolean' | 'object' | 'array' | undefined {
  if (schema.type === 'object' || objectValue(schema.properties)) return 'object'
  if (schema.type === 'array' && objectValue(schema.items)) return 'array'
  if (schema.type === 'string' || schema.type === 'number' || schema.type === 'integer' || schema.type === 'boolean') {
    return schema.type
  }
  if (Array.isArray(schema.type) && schema.type.includes('null')) {
    const types = schema.type.filter((item) => item !== 'null')
    if (types.length === 1 && ['string', 'number', 'integer', 'boolean'].includes(String(types[0]))) {
      return types[0] as 'string' | 'number' | 'integer' | 'boolean'
    }
  }
  return undefined
}

function validEnum(value: unknown): value is Array<string | number | boolean> {
  return Array.isArray(value)
    && value.length > 0
    && value.length <= 100
    && value.every((item) => typeof item === 'string' || typeof item === 'number' || typeof item === 'boolean')
}

function safeFieldName(key: string, schema: FieldSchema) {
  return !sensitiveFieldPattern.test(key.replace(/[^a-z0-9]/gi, '')) && schema.format !== 'password'
}

function schemaHasSensitiveFields(schema: unknown, depth = 0): boolean {
  if (depth > MAX_RENDER_DEPTH) return false
  const object = objectValue(schema)
  if (!object) return false
  const properties = objectValue(object.properties)
  if (properties && Object.entries(properties).some(([key, child]) => {
    const field = objectValue(child)
    return sensitiveFieldPattern.test(key.replace(/[^a-z0-9]/gi, ''))
      || field?.format === 'password'
      || schemaHasSensitiveFields(child, depth + 1)
  })) return true
  return schemaHasSensitiveFields(object.items, depth + 1)
}

function renderableField(schema: FieldSchema, depth: number): boolean {
  if (depth > MAX_RENDER_DEPTH || (schema.enum !== undefined && !validEnum(schema.enum))) return false
  switch (schemaType(schema)) {
    case 'object':
      return Object.entries(objectValue(schema.properties) ?? {}).some(([key, value]) => renderableProperty(key, value, depth + 1))
    case 'array': {
      const items = objectValue(schema.items) as FieldSchema | undefined
      return Boolean(items && safeFieldName('', items) && renderableField(items, depth + 1))
    }
    case undefined: return false
    default: return true
  }
}

function renderableProperty(key: string, value: unknown, depth: number): value is FieldSchema {
  const field = objectValue(value) as FieldSchema | undefined
  return Boolean(safeConfigKey(key) && field && safeFieldName(key, field) && renderableField(field, depth))
}

export function hasPluginConfigurationFields(schema: unknown): boolean {
  const root = objectValue(schema) as FieldSchema | undefined
  return Boolean(root && schemaType(root) === 'object' && renderableField(root, 0))
}

function propertyEntries(schema: FieldSchema, locale: string, depth: number) {
  const properties = objectValue(schema.properties)
  if (!properties) return []
  const required = new Set(Array.isArray(schema.required)
    ? schema.required.filter((item): item is string => typeof item === 'string')
    : [])
  return Object.entries(properties).flatMap(([key, field], index) => {
    if (!renderableProperty(key, field, depth)) return []
    return [{
      key,
      field,
      required: required.has(key),
      index,
      group: localizedText(field['x-ui-group'], locale),
      order: typeof field['x-ui-order'] === 'number' && Number.isFinite(field['x-ui-order']) ? field['x-ui-order'] : index,
    }]
  }).sort((left, right) => left.order - right.order || left.index - right.index).slice(0, MAX_RENDERED_FIELDS)
}

function conditionMatches(schema: FieldSchema, parent: unknown): boolean {
  const condition = objectValue(schema['x-ui-visible-when'])
  if (!condition || typeof condition.field !== 'string') return true
  if (condition.field.length > 256) return true
  const conditionPath = condition.field.split('.').filter(Boolean)
  if (conditionPath.length === 0 || conditionPath.length > MAX_RENDER_DEPTH || conditionPath.some((key) => !safeConfigKey(key))) return true
  const candidate = readPath(parent, conditionPath)
  if (Object.hasOwn(condition, 'equals')) return JSON.stringify(candidate) === JSON.stringify(condition.equals)
  if (Object.hasOwn(condition, 'notEquals')) return JSON.stringify(candidate) !== JSON.stringify(condition.notEquals)
  return true
}

function defaultForSchema(schema: FieldSchema): unknown {
  if (Object.hasOwn(schema, 'default')) return schema.default
  if (validEnum(schema.enum)) return schema.enum[0]
  switch (schemaType(schema)) {
    case 'object': return {}
    case 'array': return []
    case 'boolean': return false
    case 'string': return ''
    case 'integer':
    case 'number': return typeof schema.minimum === 'number' ? schema.minimum : 0
    default: return null
  }
}

function applyDefaults(schema: FieldSchema, value: unknown): unknown {
  const type = schemaType(schema)
  if (type === 'object') {
    const existing = objectValue(value)
    const output: JsonObject = { ...(existing ?? {}) }
    for (const [key, rawChild] of Object.entries(objectValue(schema.properties) ?? {})) {
      if (!safeConfigKey(key)) continue
      const child = objectValue(rawChild) as FieldSchema | undefined
      if (!child || !safeFieldName(key, child)) continue
      if (Object.hasOwn(output, key)) output[key] = applyDefaults(child, output[key])
      else if (Object.hasOwn(child, 'default')) output[key] = child.default
    }
    return output
  }
  if (type === 'array' && Array.isArray(value)) {
    const itemSchema = objectValue(schema.items) as FieldSchema | undefined
    return itemSchema ? value.map((item) => applyDefaults(itemSchema, item)) : [...value]
  }
  return value === undefined && Object.hasOwn(schema, 'default') ? schema.default : value
}

/** Apply declared JSON Schema defaults without inventing values for required fields. */
export function pluginConfigurationWithDefaults(schema: unknown, value: unknown): JsonObject {
  const root = objectValue(schema) as FieldSchema | undefined
  const config = objectValue(value) ?? {}
  if (!root || schemaType(root) !== 'object') return { ...config }
  return applyDefaults(root, config) as JsonObject
}

function matchesSchema(schema: FieldSchema, value: unknown, depth = 0): boolean {
  if (depth > MAX_RENDER_DEPTH) return false
  const type = schemaType(schema)
  if (value === null) return schema.type === 'null' || (Array.isArray(schema.type) && schema.type.includes('null'))
  if (type === 'object') {
    const object = objectValue(value)
    if (!object) return false
    if (Object.keys(object).some((key) => !safeConfigKey(key))) return false
    if (Object.keys(object).some((key) => sensitiveFieldPattern.test(key.replace(/[^a-z0-9]/gi, '')))) return false
    const properties = objectValue(schema.properties) ?? {}
    const required = Array.isArray(schema.required) ? schema.required : []
    if (required.some((key) => {
      if (typeof key !== 'string' || Object.hasOwn(object, key)) return false
      const propertySchema = objectValue(properties[key]) as FieldSchema | undefined
      return !propertySchema || conditionMatches(propertySchema, object)
    })) return false
    if (schema.additionalProperties === false && Object.keys(object).some((key) => !Object.hasOwn(properties, key))) return false
    return Object.entries(properties).every(([key, rawChild]) => {
      if (!Object.hasOwn(object, key)) return true
      const child = objectValue(rawChild) as FieldSchema | undefined
      return !child || matchesSchema(child, object[key], depth + 1)
    })
  }
  if (type === 'array') {
    if (!Array.isArray(value)) return false
    if (typeof schema.minItems === 'number' && value.length < schema.minItems) return false
    if (typeof schema.maxItems === 'number' && value.length > schema.maxItems) return false
    const itemSchema = objectValue(schema.items) as FieldSchema | undefined
    return itemSchema ? value.every((item) => matchesSchema(itemSchema, item, depth + 1)) : true
  }
  if (type === 'string' && typeof value !== 'string') return false
  if (type === 'boolean' && typeof value !== 'boolean') return false
  if ((type === 'number' || type === 'integer') && (typeof value !== 'number' || !Number.isFinite(value))) return false
  if (type === 'integer' && typeof value === 'number' && !Number.isInteger(value)) return false
  if (typeof value === 'string') {
    if (typeof schema.minLength === 'number' && value.length < schema.minLength) return false
    if (typeof schema.maxLength === 'number' && value.length > schema.maxLength) return false
  }
  if (typeof value === 'number') {
    if (typeof schema.minimum === 'number' && value < schema.minimum) return false
    if (typeof schema.maximum === 'number' && value > schema.maximum) return false
  }
  if (validEnum(schema.enum) && !schema.enum.some((item) => Object.is(item, value))) return false
  return true
}

export function isPluginConfigurationValid(schema: unknown, value: unknown): boolean {
  const root = objectValue(schema) as FieldSchema | undefined
  return Boolean(root && matchesSchema(root, value))
}

/** Render bounded host-owned controls. Plugin manifests provide schemas and labels only. */
export function PluginConfigEditor({ schema, value, onChange, locale }: {
  schema: unknown
  value: string
  onChange: (value: string) => void
  locale: string
}) {
  const { t } = useTranslation()
  const root = objectValue(schema) as FieldSchema | undefined
  const parsed = parseJson(value)
  const current = parsed.valid ? objectValue(parsed.value) : undefined
  const hasSensitiveSchemaFields = schemaHasSensitiveFields(root)
  const rendered = { count: 0 }
  const update = (path: JsonPath, nextValue: unknown) => {
    if (path.some((part) => typeof part === 'string' && !safeConfigKey(part))) return
    if (path.length === 0) {
      onChange(JSON.stringify(nextValue ?? null, null, 2))
      return
    }
    if (!parsed.valid) return
    onChange(JSON.stringify(writePath(parsed.value, path, nextValue), null, 2))
  }

  const labelFor = (schemaNode: FieldSchema, fallback: string) => localizedText(schemaNode['x-ui-label'], locale) ?? localizedText(schemaNode.title, locale) ?? fallback
  const descriptionFor = (schemaNode: FieldSchema) => localizedText(schemaNode['x-ui-description'], locale) ?? localizedText(schemaNode.description, locale)
  const placeholderFor = (schemaNode: FieldSchema) => localizedText(schemaNode['x-ui-placeholder'], locale)

  const renderField = (
    schemaNode: FieldSchema,
    fieldValue: unknown,
    path: JsonPath,
    requiredField: boolean,
    depth: number,
    fallbackLabel: string,
    parentValue: unknown,
  ): ReactNode => {
    if (depth > MAX_RENDER_DEPTH || rendered.count >= MAX_RENDERED_FIELDS || !conditionMatches(schemaNode, parentValue)) return null
    rendered.count += 1
    const type = schemaType(schemaNode)
    const label = labelFor(schemaNode, fallbackLabel)
    const description = descriptionFor(schemaNode)
    const placeholder = placeholderFor(schemaNode)

    if (type === 'object') {
      return renderObject(schemaNode, fieldValue, path, requiredField, depth, label, description)
    }
    if (type === 'array') {
      const itemsSchema = objectValue(schemaNode.items) as FieldSchema | undefined
      if (!itemsSchema || !schemaType(itemsSchema)) return null
      const items = Array.isArray(fieldValue) ? fieldValue : []
      const visibleItems = items.slice(0, MAX_ARRAY_ITEMS)
      const minItems = typeof schemaNode.minItems === 'number' ? schemaNode.minItems : 0
      const maxItems = typeof schemaNode.maxItems === 'number' ? Math.min(schemaNode.maxItems, MAX_ARRAY_ITEMS) : MAX_ARRAY_ITEMS
      return <fieldset key={pathKey(path)} className="grid min-w-0 gap-2 rounded-lg border border-divider p-3">
        <legend className="px-1 text-sm font-medium">{label}{requiredField ? <span className="ms-1 text-danger">*</span> : null}</legend>
        {description ? <p className="text-xs text-muted-foreground">{description}</p> : null}
        {items.length ? <div className="grid gap-2">{visibleItems.map((item, index) => <div key={`${pathKey(path)}:${index}`} className="grid min-w-0 gap-2 rounded-md bg-content1/40 p-2">
          {renderField(itemsSchema, item, [...path, index], true, depth + 1, `${label} ${index + 1}`, fieldValue)}
          <div className="flex justify-end"><Button type="button" size="sm" variant="light" color="danger" isDisabled={items.length <= minItems} aria-label={t('pluginLifecycle.configEditor.removeItem', { item: index + 1 })} onPress={() => update(path, items.filter((_, itemIndex) => itemIndex !== index))}><Trash2 size={14} />{t('pluginLifecycle.configEditor.remove')}</Button></div>
        </div>)}</div> : <p className="text-xs text-muted-foreground">{t('pluginLifecycle.configEditor.emptyList')}</p>}
        {items.length > MAX_ARRAY_ITEMS ? <p role="status" className="text-xs text-muted-foreground">{t('pluginLifecycle.configEditor.listRenderLimit', { count: MAX_ARRAY_ITEMS })}</p> : null}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={items.length >= maxItems} onPress={() => update(path, [...items, defaultForSchema(itemsSchema)])}><Plus size={14} />{t('pluginLifecycle.configEditor.addItem')}</Button>
          <span className="text-xs text-muted-foreground">{t('pluginLifecycle.configEditor.itemCount', { count: items.length, max: maxItems })}</span>
        </div>
      </fieldset>
    }

    if (validEnum(schemaNode.enum)) {
      const labels = Array.isArray(schemaNode['x-ui-enum-labels']) ? schemaNode['x-ui-enum-labels'] : []
      return <label key={pathKey(path)} className="grid min-w-0 gap-1 text-sm font-medium">
        <span>{label}{requiredField ? <span className="ms-1 text-danger">*</span> : null}</span>
        <select className="h-10 w-full rounded-md border border-divider bg-background px-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary" value={fieldValue === undefined ? '' : JSON.stringify(fieldValue)} aria-label={label} aria-required={requiredField} onChange={(event) => {
          if (!event.target.value) { update(path, undefined); return }
          try { update(path, JSON.parse(event.target.value)) } catch { update(path, event.target.value) }
        }}>
          <option value="">{t('pluginLifecycle.configEditor.choose')}</option>
          {schemaNode.enum.map((option, index) => <option key={`${pathKey(path)}:${JSON.stringify(option)}`} value={JSON.stringify(option)}>{localizedText(labels[index], locale) ?? String(option)}</option>)}
        </select>
        {description ? <small className="font-normal text-muted-foreground">{description}</small> : null}
      </label>
    }

    if (type === 'boolean') {
      return <label key={pathKey(path)} className="flex min-w-0 items-start gap-2 rounded-md border border-divider p-3 text-sm">
        <input className="mt-1 accent-primary" type="checkbox" checked={fieldValue === true} onChange={(event) => update(path, event.target.checked)} />
        <span className="min-w-0"><strong>{label}{requiredField ? <span className="ms-1 text-danger">*</span> : null}</strong>{description ? <small className="mt-1 block font-normal text-muted-foreground">{description}</small> : null}</span>
      </label>
    }

    if (type !== 'string' && type !== 'number' && type !== 'integer') return null
    const textValue = typeof fieldValue === 'string' || typeof fieldValue === 'number' ? String(fieldValue) : ''
    const inputType: 'number' | 'email' | 'url' | 'text' = type === 'number' || type === 'integer' ? 'number' : schemaNode.format === 'email' ? 'email' : schemaNode.format === 'uri' || schemaNode.format === 'url' ? 'url' : 'text'
    const onTextChange = (next: string) => {
      if (next === '') { update(path, undefined); return }
      if (type === 'number' || type === 'integer') {
        const parsed = Number(next)
        if (!Number.isFinite(parsed) || (type === 'integer' && !Number.isInteger(parsed))) return
        update(path, parsed)
      } else update(path, next)
    }
    if (type === 'string' && (schemaNode.format === 'textarea' || schemaNode.format === 'multiline')) {
      return <Textarea key={pathKey(path)} label={label} description={description} placeholder={placeholder} isRequired={requiredField} maxLength={typeof schemaNode.maxLength === 'number' ? schemaNode.maxLength : undefined} minRows={3} value={textValue} onValueChange={onTextChange} />
    }
    return <Input key={pathKey(path)} label={label} description={description} placeholder={placeholder} size="sm" type={inputType} isRequired={requiredField} min={typeof schemaNode.minimum === 'number' ? schemaNode.minimum : undefined} max={typeof schemaNode.maximum === 'number' ? schemaNode.maximum : undefined} step={type === 'integer' ? 1 : undefined} minLength={typeof schemaNode.minLength === 'number' ? schemaNode.minLength : undefined} maxLength={typeof schemaNode.maxLength === 'number' ? schemaNode.maxLength : undefined} value={textValue} onValueChange={onTextChange} />
  }

  const renderObject = (
    schemaNode: FieldSchema,
    fieldValue: unknown,
    path: JsonPath,
    requiredField: boolean,
    depth: number,
    label: string,
    description?: string,
  ): ReactNode => {
    const object = objectValue(fieldValue)
    if (path.length > 0 && !object && !requiredField) {
      return <div key={pathKey(path)} className="flex min-w-0 flex-wrap items-start justify-between gap-2 rounded-lg border border-divider p-3">
        <span className="min-w-0"><strong className="text-sm">{label}</strong>{description ? <small className="mt-1 block text-xs text-muted-foreground">{description}</small> : null}</span>
        <Button type="button" size="sm" variant="bordered" onPress={() => update(path, {})}><Plus size={14} />{t('pluginLifecycle.configEditor.enableSection')}</Button>
      </div>
    }

    const parent = object ?? {}
    const entries = propertyEntries(schemaNode, locale, depth + 1).filter(({ field }) => conditionMatches(field, parent))
    const groups = new Map<string, typeof entries>()
    for (const entry of entries) {
      const group = entry.group ?? ''
      groups.set(group, [...(groups.get(group) ?? []), entry])
    }
    const fieldRows = [...groups.entries()].map(([group, groupEntries]) => <div key={`${pathKey(path)}:group:${group}`} className="grid min-w-0 gap-3">
      {group ? <h4 className="border-b border-divider pb-1 text-xs font-semibold text-muted-foreground">{group}</h4> : null}
      {groupEntries.map(({ key, field, required: childRequired }) => renderField(field, parent[key], [...path, key], childRequired, depth + 1, key, parent))}
    </div>)

    if (path.length === 0) return fieldRows.length
      ? <div className="grid min-w-0 gap-4">{fieldRows}</div>
      : <p className="text-sm text-muted-foreground">{t('pluginLifecycle.configEditor.emptyForm')}</p>
    return <fieldset key={pathKey(path)} className="grid min-w-0 gap-3 rounded-lg border border-divider p-3">
      <legend className="max-w-full px-1 text-sm font-semibold">{label}{requiredField ? <span className="ms-1 text-danger">*</span> : null}</legend>
      {description ? <p className="text-xs text-muted-foreground">{description}</p> : null}
      {!requiredField ? <div className="flex justify-end"><Button type="button" size="sm" variant="light" color="danger" onPress={() => update(path, undefined)}><Trash2 size={14} />{t('pluginLifecycle.configEditor.removeSection')}</Button></div> : null}
      {fieldRows}
    </fieldset>
  }

  if (!root) return <p role="status" className="text-sm text-muted-foreground">{t('pluginLifecycle.configEditor.unsupportedSchema')}</p>
  if (!parsed.valid) return <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.configEditor.invalidValue')}</p>
  const rootType = schemaType(root)

  return <div className="grid min-w-0 gap-4">
    {hasSensitiveSchemaFields ? <p role="note" className="rounded-md bg-warning/10 p-3 text-xs text-warning-foreground">{t('pluginLifecycle.configEditor.sensitiveSchema')}</p> : null}
    {rootType === 'object' && current
      ? renderObject(root, current, [], true, 0, '')
      : rootType === 'object'
        ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.configEditor.invalidValue')}</p>
        : rootType
          ? renderField(root, parsed.value, [], true, 0, t('pluginLifecycle.configEditor.value'), parsed.value)
          : <p role="status" className="text-sm text-muted-foreground">{t(root.type === 'null' ? 'pluginLifecycle.configEditor.emptyForm' : 'pluginLifecycle.configEditor.unsupportedSchema')}</p>}
    {rendered.count >= MAX_RENDERED_FIELDS ? <p role="status" className="text-xs text-muted-foreground">{t('pluginLifecycle.configEditor.renderLimit')}</p> : null}
  </div>
}

function pathKey(path: JsonPath) {
  return JSON.stringify(path)
}
