import { Button, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Textarea } from '@heroui/react'
import { Package } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import {
  type PluginMarketIndex,
  useInstallMarketPlugin,
  usePluginDeploymentHistory,
  usePluginMarketIndex,
} from './plugin-lifecycle-api'

type PluginEntry = PluginMarketIndex['plugins'][number]
type MarketVersion = PluginEntry['versions'][number]

function httpsUrl(input: string | null | undefined): string | undefined {
  if (!input) return undefined
  try {
    const url = new URL(input)
    if (url.protocol === 'https:' && !url.username && !url.password && !url.hash) return url.href
  } catch {
    return undefined
  }
  return undefined
}

export function PluginMarketPanel({ disabled, onInstallingChange }: {
  disabled: boolean
  onInstallingChange: (installing: boolean) => void
}) {
  const { t } = useTranslation()
  const [input, setInput] = useState('')
  const [indexUrl, setIndexUrl] = useState('')
  const [invalidUrl, setInvalidUrl] = useState(false)
  const [search, setSearch] = useState('')
  const [entry, setEntry] = useState<PluginEntry>()
  const [notice, setNotice] = useState('')
  const market = usePluginMarketIndex(indexUrl)
  const entries = useMemo(() => {
    const query = search.trim().toLowerCase()
    return (market.data?.plugins ?? []).filter((item) =>
      [item.id, item.title, item.description, item.author].some((value) => value?.toLowerCase().includes(query)),
    )
  }, [market.data, search])
  const load = () => {
    const value = httpsUrl(input.trim())
    setInvalidUrl(!value)
    if (!value) return
    if (value === indexUrl) void market.refetch()
    else {
      setIndexUrl(value)
      setSearch('')
      setNotice('')
    }
  }
  return (
    <div className="grid gap-4">
        <form className="flex flex-col items-start gap-2 sm:flex-row sm:items-center" onSubmit={(event) => { event.preventDefault(); load() }}>
          <Input label={t('pluginLifecycle.market.indexUrl')} placeholder="https://example.org/plugins.json"
            type="url" value={input} onValueChange={(value) => { setInput(value); setInvalidUrl(false) }}
            isDisabled={disabled} isInvalid={invalidUrl} errorMessage={invalidUrl ? t('pluginLifecycle.market.invalidUrl') : undefined} />
          <Button className="shrink-0" type="submit" variant="bordered" isLoading={market.isFetching} isDisabled={disabled || !input.trim() || market.isFetching}>{t('pluginLifecycle.market.load')}</Button>
        </form>
        {notice ? <p role="status" className="text-sm text-success">{notice}</p> : null}
        {market.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.market.loadError')}</p> : null}
        {market.data ? <>
          <div>
            <h3 className="break-words text-sm font-medium">{market.data.title}</h3>
            {market.data.description ? <p className="break-words text-xs text-muted-foreground">{market.data.description}</p> : null}
          </div>
          <Input size="sm" label={t('pluginLifecycle.market.search')} value={search} onValueChange={setSearch} />
          {entries.length === 0 ? <p className="text-sm text-muted-foreground">{t('pluginLifecycle.market.empty')}</p> : null}
          <div className="grid gap-2 sm:grid-cols-2">
            {entries.map((item) => {
              const icon = httpsUrl(item.icon)
              const safeIcon = icon && new URL(icon).origin === new URL(indexUrl).origin ? icon : undefined
              return (
                <article key={item.id} className="flex min-w-0 items-start gap-3 rounded-lg border border-divider p-3">
                  <div className="grid size-8 shrink-0 place-items-center rounded-md bg-content2">
                    {safeIcon ? <img src={safeIcon} alt="" className="size-6 object-contain" loading="lazy" referrerPolicy="no-referrer" /> : <Package size={16} aria-hidden="true" />}
                  </div>
                  <div className="min-w-0 flex-1">
                    <h4 className="break-words text-sm font-medium">{item.title}</h4>
                    <p className="break-all text-xs text-muted-foreground">{item.id}{item.author ? ` · ${item.author}` : ''}</p>
                    {item.description ? <p className="mt-1 break-words text-xs">{item.description}</p> : null}
                    <Button size="sm" variant="light" className="mt-1" isDisabled={disabled || item.versions.length === 0}
                      onPress={() => setEntry(item)}>{t('pluginLifecycle.market.versions')}</Button>
                  </div>
                </article>
              )
            })}
          </div>
        </> : null}
      {entry ? <MarketInstallDialog key={entry.id} entry={entry} onClose={() => setEntry(undefined)}
        onInstallingChange={onInstallingChange}
        onInstalled={(label) => { setNotice(label); setEntry(undefined) }} /> : null}
    </div>
  )
}

function MarketInstallDialog({ entry, onClose, onInstalled, onInstallingChange }: {
  entry: PluginEntry
  onClose: () => void
  onInstalled: (message: string) => void
  onInstallingChange: (installing: boolean) => void
}) {
  const { t } = useTranslation()
  const [version, setVersion] = useState<MarketVersion | undefined>(entry.versions[0])
  const [config, setConfig] = useState('')
  const history = usePluginDeploymentHistory(entry.id)
  const install = useInstallMarketPlugin()
  useEffect(() => {
    onInstallingChange(install.isPending)
    return () => onInstallingChange(false)
  }, [install.isPending, onInstallingChange])
  const current = history.data?.[0]
  const error = install.error instanceof ApiError ? install.error.status : undefined
  const source = version?.source
  const archiveUrl = httpsUrl(source?.archiveUrl ?? version?.archiveUrl)
  const commit = source?.commit ?? version?.commit
  const changelog = httpsUrl(version?.changelog)
  const repository = httpsUrl(source?.repository ?? entry.repository)
  const parsedConfig: unknown = useMemo(() => {
    if (!config.trim()) return undefined
    try { return JSON.parse(config) } catch { return null }
  }, [config])
  const invalidConfig = parsedConfig !== undefined && (!parsedConfig || typeof parsedConfig !== 'object' || Array.isArray(parsedConfig))
  const submit = () => {
    if (!version || !archiveUrl || !history.isSuccess || history.isFetching || invalidConfig) return
    install.mutate({
      plugin_id: entry.id,
      plugin_version: version.version,
      expected_generation: current?.generation ?? null,
      ...(parsedConfig ? { config: parsedConfig as Record<string, unknown> } : {}),
      source: {
        kind: source?.kind ?? 'http_archive',
        archive_url: archiveUrl,
        sha256: source?.sha256 ?? version.sha256,
        repository,
        commit: source?.commit,
        path: source?.path,
      },
    }, { onSuccess: () => onInstalled(t('pluginLifecycle.install.success', { plugin: entry.id, version: version.version })) })
  }
  return (
    <Modal isOpen isDismissable={!install.isPending} isKeyboardDismissDisabled={install.isPending} scrollBehavior="inside"
      onOpenChange={(open) => { if (!open && !install.isPending) onClose() }}>
      <ModalContent>
        <ModalHeader className="break-words">{entry.title}</ModalHeader>
        <ModalBody>
          <p className="text-xs text-muted-foreground">{t('pluginLifecycle.trustWarning')}</p>
          <label className="grid gap-1 text-sm">
            {t('pluginLifecycle.market.selectVersion')}
            <select className="w-full rounded-md border border-divider bg-background px-2 py-2" value={version?.version} disabled={install.isPending}
              onChange={(event) => { setVersion(entry.versions.find((item) => item.version === event.target.value)); install.reset() }}>
              {entry.versions.map((item) => <option key={item.version} value={item.version}>{item.version}</option>)}
            </select>
          </label>
          {current ? <p className="text-sm">{t('pluginLifecycle.market.installed', { version: current.artifact.pluginVersion })}</p> : null}
          {version?.runtime ? <p className="break-words text-sm">{t('pluginLifecycle.market.runtime', { runtime: version.runtime })}</p> : null}
          {version?.capabilities.length ? <p className="break-words text-xs text-muted-foreground">{version.capabilities.join(', ')}</p> : null}
          {version ? (
            <dl className="space-y-1 text-xs text-muted-foreground">
              <div><dt className="inline">SHA-256 </dt><dd className="inline break-all font-mono">{source?.sha256 ?? version.sha256}</dd></div>
              {commit ? <div><dt className="inline">Commit </dt><dd className="inline break-all font-mono">{commit}</dd></div> : null}
            </dl>
          ) : null}
          <div className="flex gap-3 text-sm">
            {repository ? <a href={repository} target="_blank" rel="noreferrer" className="text-primary underline">{t('pluginLifecycle.market.repository')}</a> : null}
            {changelog ? <a href={changelog} target="_blank" rel="noreferrer" className="text-primary underline">{t('pluginLifecycle.market.changelog')}</a> : null}
          </div>
          {version?.changelog && !changelog ? <p className="whitespace-pre-wrap break-words text-sm">{version.changelog}</p> : null}
          <details className="text-sm">
            <summary className="cursor-pointer">{t('pluginLifecycle.install.configure')}</summary>
            <Textarea label={t('pluginLifecycle.fields.config')} value={config} onValueChange={setConfig} placeholder="{}"
              isDisabled={install.isPending} isInvalid={invalidConfig} errorMessage={invalidConfig ? t('pluginLifecycle.install.errors.configJson') : undefined}
              description={t('pluginLifecycle.market.configHint')} className="mt-2" />
          </details>
          {history.isError ? <button type="button" className="self-start text-sm text-danger underline" onClick={() => void history.refetch()}>{t('pluginLifecycle.market.historyError')}</button> : null}
          {install.isError ? <p role="alert" className="text-sm text-danger">{t(error === 422 ? 'pluginLifecycle.market.needsReview' : error === 409 ? 'pluginLifecycle.install.errors.conflict' : 'pluginLifecycle.market.installError')}</p> : null}
        </ModalBody>
        <ModalFooter>
          <Button variant="light" onPress={onClose} isDisabled={install.isPending}>{t('pluginLifecycle.cancel')}</Button>
          <Button color="primary" isLoading={install.isPending} isDisabled={!archiveUrl || invalidConfig || !history.isSuccess || history.isFetching || error === 422 || (current?.enabled && current.artifact.pluginVersion === version?.version)} onPress={submit}>
            {t(current ? 'pluginLifecycle.market.update' : 'pluginLifecycle.install.submit')}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  )
}
