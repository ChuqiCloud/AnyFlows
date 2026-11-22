import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem, Textarea } from '@heroui/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useAdminChannels } from '@/features/channels/channel-api'
import { ApiError } from '@/lib/api'
import type { PluginDeploymentResponse, PluginProviderSelectionBody } from '@/lib/api/generated/types.gen'
import { useConfigurePluginProvider } from './plugin-lifecycle-api'

export function PluginProviderEditor({ deployment, onClose }: {
  deployment: PluginDeploymentResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const channels = useAdminChannels()
  const save = useConfigurePluginProvider()
  const [generation] = useState(deployment.generation)
  const [selected, setSelected] = useState(deployment.providerSelection?.channelIds ?? [])
  const [models, setModels] = useState(deployment.providerSelection?.models.join('\n') ?? '')
  const [priority, setPriority] = useState(String(deployment.providerSelection?.priority ?? 0))
  const [upstreamResponseMode, setUpstreamResponseMode] = useState<NonNullable<PluginProviderSelectionBody['upstreamResponseMode']>>(
    deployment.providerSelection?.upstreamResponseMode ?? 'auto',
  )
  const [search, setSearch] = useState('')
  const modelList = [...new Set(models.split('\n').map((model) => model.trim()).filter(Boolean))]
  const priorityValue = Number(priority)
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
          <Textarea label={t('pluginLifecycle.provider.models')} description={t('pluginLifecycle.provider.modelsHint')} value={models} onValueChange={setModels} isDisabled={save.isPending} minRows={2} />
          <Input label={t('pluginLifecycle.provider.priority')} type="number" value={priority} onValueChange={setPriority} isDisabled={save.isPending} />
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
          <Button color="primary" isLoading={save.isPending} isDisabled={stale || !priorityValid || selected.length > 128 || modelList.length > 128}
            onPress={() => save.mutate({
              pluginId: deployment.pluginId,
              body: { expected_generation: generation, selection: { channelIds: selected, models: modelList, priority: priorityValue, upstreamResponseMode } },
            }, { onSuccess: onClose })}>{t('pluginLifecycle.provider.save')}</Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
