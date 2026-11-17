import { Button, Chip, Select, SelectItem } from '@heroui/react'
import { Play } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import { PluginConfigEditor } from './plugin-config-editor'
import { usePluginCustomCapability } from './plugin-lifecycle-api'

type Capability = {
  id: string
  version: number
  inputSchema: Record<string, unknown>
  outputSchema: Record<string, unknown>
}

function object(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

function capabilities(deployment: PluginDeploymentResponse): Capability[] {
  if (!deployment.enabled || !deployment.artifact.approval || deployment.artifact.revocation) return []
  const manifest = object(deployment.artifact.manifest)
  const declared = manifest?.capabilities
  if (!Array.isArray(declared)) return []
  const granted = new Set(deployment.grantedCapabilities.map((item) => `${item.id}@${item.version}`))
  return declared.flatMap((candidate): Capability[] => {
    const item = object(candidate)
    if (!item || item.access !== 'read' || item.sideEffect !== 'none'
      || typeof item.id !== 'string' || typeof item.version !== 'number'
      || !granted.has(`${item.id}@${item.version}`)) return []
    const inputSchema = object(item.inputSchema)
    const outputSchema = object(item.outputSchema)
    return inputSchema && outputSchema
      ? [{ id: item.id, version: item.version, inputSchema, outputSchema }]
      : []
  })
}

function parseInput(value: string): Record<string, unknown> | undefined {
  try {
    return object(JSON.parse(value))
  } catch {
    return undefined
  }
}

export function PluginCapabilityConsole({ deployment }: { deployment: PluginDeploymentResponse }) {
  const { t, i18n } = useTranslation()
  const callable = capabilities(deployment)
  const [capabilityId, setCapabilityId] = useState(callable[0]?.id ?? '')
  const [input, setInput] = useState('{}')
  const [output, setOutput] = useState<unknown>()
  const invocation = usePluginCustomCapability()
  const selected = callable.find((item) => item.id === capabilityId) ?? callable[0]
  const parsed = parseInput(input)

  if (callable.length === 0) return null

  return <details className="mt-3 rounded-lg border border-[var(--hairline)] p-3">
    <summary className="cursor-pointer text-sm font-medium">{t('pluginLifecycle.capabilityConsole.title')}</summary>
    <div className="mt-3 grid gap-3">
      <p className="text-xs text-muted-foreground">{t('pluginLifecycle.capabilityConsole.description')}</p>
      <Select
        label={t('pluginLifecycle.capabilityConsole.capability')}
        selectedKeys={selected ? [selected.id] : []}
        onSelectionChange={(keys) => {
          setCapabilityId(String(Array.from(keys)[0] ?? ''))
          setInput('{}')
          setOutput(undefined)
        }}
        disallowEmptySelection
      >
        {callable.map((capability) => <SelectItem key={capability.id} textValue={capability.id}>
          {capability.id} · v{capability.version}
        </SelectItem>)}
      </Select>
      {selected ? <PluginConfigEditor
        schema={selected.inputSchema}
        value={input}
        onChange={(next) => { setInput(next); setOutput(undefined) }}
        locale={i18n.resolvedLanguage ?? 'en'}
      /> : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          color="primary"
          isDisabled={!selected || !parsed || invocation.isPending}
          onPress={() => {
            if (!selected || !parsed) return
            invocation.mutate({
              pluginId: deployment.pluginId,
              capabilityId: selected.id,
              capabilityVersion: selected.version,
              input: parsed,
            }, { onSuccess: (result) => setOutput(result.output) })
          }}
        >
          <Play size={14} />{t(invocation.isPending ? 'pluginLifecycle.capabilityConsole.running' : 'pluginLifecycle.capabilityConsole.run')}
        </Button>
        {output !== undefined ? <Chip size="sm" variant="flat" color="success">{t('pluginLifecycle.capabilityConsole.complete')}</Chip> : null}
      </div>
      {invocation.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.capabilityConsole.error')}</p> : null}
      {output !== undefined ? <pre className="max-h-80 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-content1/50 p-3 font-mono text-xs">{JSON.stringify(output, null, 2)}</pre> : null}
    </div>
  </details>
}
