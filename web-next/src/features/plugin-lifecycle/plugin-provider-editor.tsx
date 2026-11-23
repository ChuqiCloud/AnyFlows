import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem } from '@heroui/react'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useAdminChannels } from '@/features/channels/channel-api'
import { ApiError } from '@/lib/api'
import type { PluginDeploymentResponse, PluginProviderSelectionBody } from '@/lib/api/generated/types.gen'
import { useConfigurePluginProvider } from './plugin-lifecycle-api'

type ProviderEndpointGrant = {
  id: string
  url: string
  allowedPaths: string[]
  allowedMethods: string[]
  websocketEnabled: boolean
}

const httpMethods = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']
const byteLength = (value: string) => new TextEncoder().encode(value).length

function validPath(path: string) {
  return byteLength(path) <= 2048 && path.startsWith('/') && !path.startsWith('//')
    && !/[?#\\%\s\p{Cc}]/u.test(path)
    && !path.split('/').some((segment) => segment === '.' || segment === '..')
}

function validOrigin(value: string) {
  try {
    const url = new URL(value)
    const ipAddress = /^\[.*\]$/.test(url.hostname) || /^\d{1,3}(?:\.\d{1,3}){3}$/.test(url.hostname)
    return (url.protocol === 'https:' || url.protocol === 'wss:') && !ipAddress
      && !url.username && !url.password && !/[?#]/.test(value)
      && (url.pathname === '/' || url.pathname === '') && url.port !== '0'
  } catch {
    return false
  }
}

function websocketOnly(url: string) {
  return /^wss:/i.test(url.trim())
}

function ListField({ label, addLabel, removeLabel, hint, invalidMessage, max, validate, values, onChange, disabled }: {
  label: string
  addLabel: string
  removeLabel: (value: string) => string
  hint?: string
  invalidMessage: string
  max: number
  validate: (value: string) => boolean
  values: string[]
  onChange: (values: string[]) => void
  disabled: boolean
}) {
  const [draft, setDraft] = useState('')
  const value = draft.trim()
  const invalid = Boolean(value && (!validate(value) || values.includes(value)))
  const add = () => {
    if (disabled || !value || invalid || values.length >= max) return
    onChange([...values, value])
    setDraft('')
  }

  return <fieldset className="grid min-w-0 gap-2" disabled={disabled}>
    <div className="flex items-start gap-2">
      <Input label={label} value={draft} onValueChange={setDraft} description={hint}
        isInvalid={invalid} errorMessage={invalid ? invalidMessage : undefined}
        isDisabled={disabled || values.length >= max} onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
            event.preventDefault()
            add()
          }
        }} />
      <Button className="mt-2 shrink-0" size="sm" variant="bordered" isDisabled={disabled || !value || invalid || values.length >= max} onPress={add}>{addLabel}</Button>
    </div>
    {values.length > 0 ? <div className="flex flex-wrap gap-1">
      {values.map((item, index) => <button type="button" key={`${item}:${index}`}
        className="max-w-full break-all rounded-md border border-divider px-2 py-1 text-xs"
        aria-label={removeLabel(item)} onClick={() => onChange(values.filter((_, position) => position !== index))}>
        {item} ×
      </button>)}
    </div> : null}
  </fieldset>
}

export function PluginProviderEditor({ deployment, onClose }: {
  deployment: PluginDeploymentResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const channels = useAdminChannels()
  const save = useConfigurePluginProvider()
  const [generation] = useState(deployment.generation)
  const [selected, setSelected] = useState(deployment.providerSelection?.channelIds ?? [])
  const [models, setModels] = useState(deployment.providerSelection?.models ?? [])
  const [priority, setPriority] = useState(String(deployment.providerSelection?.priority ?? 0))
  const [upstreamResponseMode, setUpstreamResponseMode] = useState<NonNullable<PluginProviderSelectionBody['upstreamResponseMode']>>(
    deployment.providerSelection?.upstreamResponseMode ?? 'auto',
  )
  const [sessionEnabled, setSessionEnabled] = useState(deployment.providerSelection?.sessionEnabled ?? false)
  const [sessionTimeoutMs, setSessionTimeoutMs] = useState(String(deployment.providerSelection?.sessionTimeoutMs ?? 30_000))
  const [endpoints, setEndpoints] = useState(() => (deployment.providerSelection?.endpoints ?? []).map((endpoint, key) => ({
    key,
    id: endpoint.id,
    url: endpoint.url,
    allowedPaths: endpoint.allowedPaths ?? ['/'],
    allowedMethods: endpoint.allowedMethods ?? [],
    websocketEnabled: endpoint.websocketEnabled ?? websocketOnly(endpoint.url),
  })))
  const nextEndpointKey = useRef(endpoints.length)
  const changeEndpoint = (key: number, changes: Partial<ProviderEndpointGrant>) =>
    setEndpoints((current) => current.map((endpoint) => endpoint.key === key ? { ...endpoint, ...changes } : endpoint))
  const [search, setSearch] = useState('')
  const priorityValue = Number(priority)
  const sessionTimeoutValue = Number(sessionTimeoutMs)
  const sessionTimeoutValid = /^\d+$/.test(sessionTimeoutMs) && sessionTimeoutValue >= 1_000 && sessionTimeoutValue <= 360_000
  const endpointList: ProviderEndpointGrant[] = endpoints.map(({ id, url, allowedPaths, allowedMethods, websocketEnabled }) => ({
    id: id.trim(), url: url.trim(), allowedPaths,
    allowedMethods: websocketOnly(url) ? [] : allowedMethods,
    websocketEnabled: websocketOnly(url) || websocketEnabled,
  }))
  const endpointIds = endpointList.map((endpoint) => endpoint.id)
  const endpointGrantsValid = !sessionEnabled || (endpointList.length <= 16
    && endpointList.every(({ id, url, allowedPaths, allowedMethods, websocketEnabled }) =>
      /^[a-z][a-z0-9_.-]{0,63}$/.test(id) && validOrigin(url)
      && allowedPaths.length > 0 && allowedPaths.length <= 64 && allowedPaths.every(validPath)
      && (allowedMethods.length > 0 || websocketEnabled))
    && new Set(endpointIds).size === endpointIds.length)
  const priorityValid = /^-?\d+$/.test(priority) && priorityValue >= -2147483648 && priorityValue <= 2147483647
  const stale = generation !== deployment.generation || (save.error instanceof ApiError && save.error.status === 409)
  const entries = channels.data?.pages.flatMap((page) => page.channels) ?? []
  const name = (id: number) => entries.find((channel) => channel.id === id)?.name
  const query = search.trim().toLowerCase()
  const displayed = entries.filter((channel) => channel.name.toLowerCase().includes(query) || String(channel.id).includes(query))
  const toggle = (id: number) => setSelected((ids) => ids.includes(id) ? ids.filter((existing) => existing !== id) : [...ids, id])
  return (
    <Modal isOpen isDismissable={!save.isPending} isKeyboardDismissDisabled={save.isPending} scrollBehavior="inside" size="2xl"
      onOpenChange={(open) => { if (!open && !save.isPending) onClose() }}>
      <ModalContent>
        <ModalHeader>{t('pluginLifecycle.provider.title')}</ModalHeader>
        <ModalBody>
          <p className="break-words text-sm font-medium">{deployment.pluginId}</p>
          <p className="text-sm text-muted-foreground">{t('pluginLifecycle.provider.description')}</p>
          <Input label={t('pluginLifecycle.provider.findChannel')} value={search} onValueChange={setSearch} />
          <fieldset className="max-h-44 space-y-1 overflow-auto rounded-md border border-divider p-2" disabled={save.isPending}>
            <legend className="px-1 text-xs">{t('pluginLifecycle.provider.channels')}</legend>
            {displayed.map((channel) => (
              <label className="flex items-center gap-2 py-1 text-sm" key={channel.id}>
                <input type="checkbox" checked={selected.includes(channel.id)} onChange={() => toggle(channel.id)} />
                <span className="min-w-0 break-words">{channel.name} · #{channel.id}</span>
              </label>
            ))}
            {!channels.isPending && !channels.isError && displayed.length === 0 ? <p className="text-xs text-muted-foreground">{t('pluginLifecycle.provider.noChannels')}</p> : null}
          </fieldset>
          {channels.isError ? <button type="button" className="self-start text-sm text-danger underline" onClick={() => void channels.refetch()}>{t('pluginLifecycle.provider.channelError')}</button> : null}
          {channels.hasNextPage ? <Button size="sm" variant="light" isLoading={channels.isFetchingNextPage} onPress={() => void channels.fetchNextPage()}>{t('pluginLifecycle.loadMore')}</Button> : null}
          {selected.length ? <div className="flex flex-wrap gap-1 text-xs">
            {selected.map((id) => <button type="button" key={id} disabled={save.isPending} className="rounded-md border border-divider px-2 py-1" aria-label={t('pluginLifecycle.provider.removeChannel', { id })} onClick={() => toggle(id)}>{name(id) ?? `#${id}`} ×</button>)}
          </div> : null}
          <ListField label={t('pluginLifecycle.provider.models')} hint={t('pluginLifecycle.provider.modelsHint')}
            addLabel={t('pluginLifecycle.provider.addModel')} removeLabel={(model) => t('pluginLifecycle.provider.removeModel', { model })}
            invalidMessage={t('pluginLifecycle.provider.modelInvalid')} max={128}
            validate={(model) => byteLength(model) <= 255 && !/\p{Cc}/u.test(model)}
            values={models} onChange={setModels} disabled={save.isPending} />
          <Input label={t('pluginLifecycle.provider.priority')} type="number" value={priority} onValueChange={setPriority} isDisabled={save.isPending} />
          <label className="flex items-start gap-2 rounded-md border border-divider p-3 text-sm">
            <input type="checkbox" checked={sessionEnabled} onChange={(event) => setSessionEnabled(event.target.checked)} disabled={save.isPending} />
            <span>
              <span className="block font-medium">{t('pluginLifecycle.provider.sessionEnabled')}</span>
              <span className="block text-xs text-muted-foreground">{t('pluginLifecycle.provider.sessionEnabledHint')}</span>
            </span>
          </label>
          {sessionEnabled ? <>
            <Input label={t('pluginLifecycle.provider.sessionTimeout')} type="number" value={sessionTimeoutMs} onValueChange={setSessionTimeoutMs} isDisabled={save.isPending} />
            <section aria-label={t('pluginLifecycle.provider.endpointGrants')} className="grid gap-3">
              <div>
                <h3 className="text-sm font-medium">{t('pluginLifecycle.provider.endpointGrants')}</h3>
                <p className="text-xs text-muted-foreground">{t('pluginLifecycle.provider.endpointGrantsHint')}</p>
              </div>
              {endpoints.map((endpoint, index) => <fieldset key={endpoint.key} disabled={save.isPending} className="grid gap-3 rounded-lg border border-divider p-3">
                <legend className="px-1 text-sm">{t('pluginLifecycle.provider.endpointNumber', { number: index + 1 })}</legend>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Input label={t('pluginLifecycle.provider.endpointId')} value={endpoint.id} maxLength={64} isDisabled={save.isPending}
                    onValueChange={(id) => changeEndpoint(endpoint.key, { id })} />
                  <Input label={t('pluginLifecycle.provider.endpointUrl')} type="url" placeholder="https://api.example.com" value={endpoint.url} isDisabled={save.isPending}
                    onValueChange={(url) => changeEndpoint(endpoint.key, { url })} />
                </div>
                <ListField label={t('pluginLifecycle.provider.endpointPath')} hint={t('pluginLifecycle.provider.endpointPathHint')}
                  addLabel={t('pluginLifecycle.provider.addPath')} removeLabel={(path) => t('pluginLifecycle.provider.removePath', { path })}
                  invalidMessage={t('pluginLifecycle.provider.pathInvalid')} max={64} validate={validPath}
                  values={endpoint.allowedPaths} onChange={(allowedPaths) => changeEndpoint(endpoint.key, { allowedPaths })} disabled={save.isPending} />
                {websocketOnly(endpoint.url) ? null : <fieldset className="grid gap-2">
                  <legend className="text-xs font-medium">{t('pluginLifecycle.provider.httpMethods')}</legend>
                  <div className="flex flex-wrap gap-3">
                    {httpMethods.map((method) => <label key={method} className="flex items-center gap-1.5 text-sm">
                      <input type="checkbox" checked={endpoint.allowedMethods.includes(method)} onChange={(event) => changeEndpoint(endpoint.key, {
                        allowedMethods: event.target.checked ? [...endpoint.allowedMethods, method] : endpoint.allowedMethods.filter((value) => value !== method),
                      })} />{method}
                    </label>)}
                  </div>
                </fieldset>}
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={websocketOnly(endpoint.url) || endpoint.websocketEnabled} disabled={websocketOnly(endpoint.url)}
                    onChange={(event) => changeEndpoint(endpoint.key, { websocketEnabled: event.target.checked })} />
                  {t('pluginLifecycle.provider.allowWebsocket')}
                </label>
                <Button size="sm" variant="light" color="danger" className="justify-self-end" isDisabled={save.isPending}
                  onPress={() => setEndpoints((current) => current.filter(({ key }) => key !== endpoint.key))}>{t('pluginLifecycle.provider.removeEndpoint')}</Button>
              </fieldset>)}
              <Button size="sm" variant="bordered" className="justify-self-start" isDisabled={save.isPending || endpoints.length >= 16}
                onPress={() => {
                  const key = nextEndpointKey.current++
                  setEndpoints((current) => [...current, { key, id: '', url: '', allowedPaths: [], allowedMethods: [], websocketEnabled: false }])
                }}>{t('pluginLifecycle.provider.addEndpoint')}</Button>
            </section>
            {!endpointGrantsValid ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.provider.endpointGrantsInvalid')}</p> : null}
          </> : null}
          <div className="grid gap-1.5">
            <label id="plugin-provider-response-mode-label" className="text-xs font-medium leading-none text-foreground">
              {t('pluginLifecycle.provider.upstreamResponseMode')}
            </label>
            <Select
              aria-labelledby="plugin-provider-response-mode-label"
              isDisabled={save.isPending}
              selectedKeys={[upstreamResponseMode]}
              onSelectionChange={(keys) => {
                const value = Array.from(keys)[0]
                if (value === 'auto' || value === 'non_streaming' || value === 'streaming') setUpstreamResponseMode(value)
              }}
            >
              <SelectItem key="auto">{t('pluginLifecycle.provider.responseModes.auto')}</SelectItem>
              <SelectItem key="non_streaming">{t('pluginLifecycle.provider.responseModes.nonStreaming')}</SelectItem>
              <SelectItem key="streaming">{t('pluginLifecycle.provider.responseModes.streaming')}</SelectItem>
            </Select>
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('pluginLifecycle.provider.upstreamResponseModeHint')}</p>
          </div>
          {stale || save.isError ? <p role="alert" className="text-sm text-danger">{t(stale ? 'pluginLifecycle.provider.conflict' : 'pluginLifecycle.provider.saveError')}</p> : null}
        </ModalBody>
        <ModalFooter>
          <Button variant="light" onPress={onClose} isDisabled={save.isPending}>{t('pluginLifecycle.cancel')}</Button>
          <Button color="primary" isLoading={save.isPending} isDisabled={stale || !priorityValid || !sessionTimeoutValid || !endpointGrantsValid || selected.length > 128 || models.length > 128}
            onPress={() => save.mutate({
              pluginId: deployment.pluginId,
              body: { expected_generation: generation, selection: { channelIds: selected, models, priority: priorityValue, upstreamResponseMode, sessionEnabled, sessionTimeoutMs: sessionTimeoutValue, endpoints: sessionEnabled ? endpointList : [] } },
            }, { onSuccess: onClose })}>{t('pluginLifecycle.provider.save')}</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
