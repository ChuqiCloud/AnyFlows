import { Button, Card, CardBody, CardHeader, Chip, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Skeleton } from '@heroui/react'
import { Check, Clock3, Plus, RefreshCw, Search, Upload } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'
import { Link } from 'react-router-dom'

import type { PluginArtifactResponse, PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import { isPluginConfigurationValid, pluginConfigurationWithDefaults, PluginConfigEditor } from './plugin-config-editor'
import { PluginCapabilityConsole } from './plugin-capability-console'
import { PluginInstallPanel } from './plugin-install-panel'
import { PluginProviderEditor } from './plugin-provider-editor'
import { PluginRuntimePanel } from './plugin-runtime-panel'
import { PluginTaskReconciliationPanel } from './plugin-task-reconciliation-panel'
import { hasRenderablePluginUiViews, PluginUiViewsRenderer } from './plugin-ui-views'
import { pluginPages } from './plugin-pages'
import {
  useApprovePluginArtifact,
  useDeployPluginArtifact,
  useDisablePluginDeployment,
  usePluginArtifacts,
  usePluginDeploymentHistory,
  usePluginDeployments,
  usePluginPublicConfiguration,
  usePluginEventResults,
  useRegisterPluginArtifact,
  useRevokePluginArtifact,
  useRollbackPluginDeployment,
  useUninstallPlugin,
  useCleanupPluginArtifact,
  useUpdatePluginConfiguration,
} from './plugin-lifecycle-api'

type ArtifactRegistrationDraft = {
  plugin_id: string
  plugin_version: string
  package_path: string
  executable_path: string
  sha256: string
  signer_key_id: string
  signature_base64: string
  manifest?: File
}

const emptyRegistration: ArtifactRegistrationDraft = {
  plugin_id: '', plugin_version: '', package_path: '', executable_path: '', sha256: '',
  signer_key_id: '', signature_base64: '',
}

const MAX_MANIFEST_FILE_BYTES = 64 * 1024

function capabilities(artifact: PluginArtifactResponse) {
  const items = artifact.manifest.capabilities
  if (!Array.isArray(items)) return []
  return items.flatMap((item) => {
    if (!item || typeof item !== 'object') return []
    const row = item as Record<string, unknown>
    if (typeof row.id !== 'string' || typeof row.version !== 'number') return []
    return [{ id: row.id, version: row.version, access: row.access, sideEffect: row.sideEffect }]
  })
}

function grantableCapabilities(artifact: PluginArtifactResponse) {
  return capabilities(artifact).filter((item) =>
    ((item.access === 'read' && item.sideEffect === 'none')
      || (item.access === 'write' && item.sideEffect === 'external_write'))
      && item.id !== 'external.account.profile.read'
      && item.id !== 'external.account.balance.read',
  )
}

function prettyDate(value: number, locale: string) {
  return new Date(value * 1_000).toLocaleString(locale)
}

type PluginUiCard = { id: string; title: string; description?: string }

function pluginUiText(value: unknown, locale: string): string | undefined {
  if (typeof value === 'string') return safePluginText(value)
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined
  const localized = value as Record<string, unknown>
  if (Object.keys(localized).some((key) => key !== 'en' && key !== 'zh')) return undefined
  const texts = Object.values(localized)
  if (texts.length < 1 || texts.length > 2 || texts.some((text) => typeof text !== 'string' || !safePluginText(text))) return undefined
  const preferred = locale.toLowerCase().startsWith('zh') ? localized.zh : localized.en
  const fallback = localized.en ?? localized.zh
  return typeof preferred === 'string' ? preferred : typeof fallback === 'string' ? fallback : undefined
}

function safePluginText(value: string): string | undefined {
  if (!value.trim() || value.length > 512
    || [...value].some((character) => {
      const codePoint = character.codePointAt(0) ?? 0
      return codePoint <= 0x1f || codePoint === 0x7f || character === '<' || character === '>'
    })
    || /(?:https?:\/\/|javascript:|data:|mailto:|www\.)/iu.test(value)) return undefined
  return value.trim()
}

/** Parse a fixed, text-only contribution format; no plugin-provided code or links are accepted. */
function pluginUiCards(deployment: PluginDeploymentResponse, locale: string): PluginUiCard[] {
  if (!deployment.enabled || !deployment.artifact.approval || deployment.artifact.revocation) return []
  const contributions = deployment.artifact.manifest.uiContributions
  if (!Array.isArray(contributions) || contributions.length > 16) return []
  const ids = new Set<string>()
  return contributions.flatMap((candidate): PluginUiCard[] => {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return []
    const item = candidate as Record<string, unknown>
    const keys = Object.keys(item)
    if (keys.some((key) => !['id', 'slot', 'component', 'title', 'description'].includes(key))
      || typeof item.id !== 'string'
      || !/^[a-z0-9](?:[a-z0-9_-]{0,62}[a-z0-9])?$/u.test(item.id)
      || ids.has(item.id)
      || item.slot !== 'plugin_management'
      || item.component !== 'info_card') return []
    ids.add(item.id)
    const title = pluginUiText(item.title, locale)
    const description = item.description === undefined ? undefined : pluginUiText(item.description, locale)
    if (!title || (item.description !== undefined && !description)) return []
    return [{ id: item.id, title, ...(description ? { description } : {}) }]
  })
}

function pluginDescription(deployment: PluginDeploymentResponse, cards: PluginUiCard[], locale: string, fallback: string): string {
  const manifest = deployment.artifact.manifest
  return pluginUiText(manifest.description, locale)
    ?? cards.find((card) => card.description)?.description
    ?? fallback
}

function PluginEventResultsPanel() {
  const { t, i18n } = useTranslation()
  const results = usePluginEventResults()
  const entries = results.data?.pages.flatMap((page) => page.results) ?? []
  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'
  return <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
    <CardHeader className="flex items-start justify-between gap-3 p-5">
      <div><h2 className="text-base font-semibold">{t('pluginLifecycle.events.title')}</h2><p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.events.description')}</p></div>
      <Button size="sm" variant="bordered" isDisabled={results.isFetching} onPress={() => { void results.refetch() }}><RefreshCw size={15} className={results.isFetching ? 'animate-spin' : undefined} />{t('pluginLifecycle.refresh')}</Button>
    </CardHeader>
    <CardBody className="space-y-3 p-5 pt-0">
      {results.isPending ? <Skeleton className="h-20 rounded-lg" /> : null}
      {results.isError ? <ErrorBox text={t('pluginLifecycle.events.error')} retry={t('pluginLifecycle.refresh')} onRetry={() => { if (results.isFetchNextPageError) void results.fetchNextPage(); else void results.refetch() }} /> : null}
      {results.isSuccess && entries.length === 0 ? <Empty text={t('pluginLifecycle.events.empty')} /> : null}
      {entries.map((entry) => <article key={entry.deliveryId} className="rounded-lg border border-[var(--hairline)] p-4">
        <div className="flex flex-wrap items-start justify-between gap-2"><div><strong className="text-sm">{entry.summary}</strong><p className="mt-1 break-all text-xs text-muted-foreground">#{entry.deliveryId} · {entry.pluginId} · {entry.eventKind} · g{entry.deploymentGeneration}</p></div><Chip size="sm" color="success" variant="flat">{t('pluginLifecycle.events.delivered')}</Chip></div>
        <p className="mt-2 text-xs text-muted-foreground">{t('pluginLifecycle.events.completedAt', { time: new Date(entry.completedAt * 1_000).toLocaleString(locale) })} · {t('pluginLifecycle.events.attempts', { count: entry.attemptCount })}</p>
        <details className="mt-3 text-xs"><summary className="cursor-pointer font-medium">{t('pluginLifecycle.events.result')}</summary><pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap break-all rounded bg-content1/50 p-3 font-mono">{JSON.stringify(entry.result, null, 2)}</pre></details>
      </article>)}
      {results.hasNextPage ? <div className="flex justify-center"><Button size="sm" variant="bordered" isDisabled={results.isFetching} onPress={() => void results.fetchNextPage()}>{t(results.isFetchingNextPage ? 'pluginLifecycle.loading' : 'pluginLifecycle.events.more')}</Button></div> : null}
    </CardBody>
  </Card>
}

export function PluginLifecyclePage() {
  const { t, i18n } = useTranslation()
  const artifacts = usePluginArtifacts()
  const deployments = usePluginDeployments()
  const register = useRegisterPluginArtifact()
  const approve = useApprovePluginArtifact()
  const revoke = useRevokePluginArtifact()
  const deploy = useDeployPluginArtifact()
  const updateConfiguration = useUpdatePluginConfiguration()
  const disable = useDisablePluginDeployment()
  const rollback = useRollbackPluginDeployment()
  const uninstall = useUninstallPlugin()
  const cleanup = useCleanupPluginArtifact()
  const [draft, setDraft] = useState(emptyRegistration)
  const [registrationOpen, setRegistrationOpen] = useState(false)
  const [registrationError, setRegistrationError] = useState(false)
  const [readingManifest, setReadingManifest] = useState(false)
  const [grantTarget, setGrantTarget] = useState<PluginArtifactResponse>()
  const [configurationMode, setConfigurationMode] = useState(false)
  const [grants, setGrants] = useState<string[]>([])
  const [config, setConfig] = useState('{}')
  const [configurationDraft, setConfigurationDraft] = useState<{ value: string; generation: number }>()
  const parsedConfiguration = (() => { try { return JSON.parse(config) as unknown } catch { return undefined } })()
  const configurationSchemaValid = Boolean(grantTarget && isPluginConfigurationValid(grantTarget.manifest.configSchema, parsedConfiguration))
  const [revocationTarget, setRevocationTarget] = useState<number>()
  const [revocationReason, setRevocationReason] = useState('')
  const [historyPlugin, setHistoryPlugin] = useState<string>()
  const [generationRefresh, setGenerationRefresh] = useState(0)
  const [installing, setInstalling] = useState(false)
  const [deploymentFilter, setDeploymentFilter] = useState('')
  const [confirmation, setConfirmation] = useState<
    { action: 'uninstall'; pluginId: string; generation: number }
    | { action: 'cleanup'; artifactId: number; pluginId: string }
  >()
  const [uninstallNotice, setUninstallNotice] = useState<number>()
  const [cleanupNotice, setCleanupNotice] = useState(false)
  const history = usePluginDeploymentHistory(historyPlugin)
  const deploymentHistory = usePluginDeploymentHistory(grantTarget?.pluginId, generationRefresh)
  const pluginConfiguration = usePluginPublicConfiguration(grantTarget?.pluginId, configurationMode)
  const artifactList = artifacts.data?.pages.flatMap((page) => page.artifacts) ?? []
  const deploymentList = deployments.data?.pages.flatMap((page) => page.deployments) ?? []
  const filter = deploymentFilter.trim().toLowerCase()
  const visibleDeployments = deploymentList.filter(({ pluginId, artifact }) =>
    pluginId.toLowerCase().includes(filter) || artifact.pluginVersion.toLowerCase().includes(filter),
  )
  const generationKnown = deploymentHistory.isSuccess && !deploymentHistory.isFetching
  const currentGeneration = deploymentHistory.data?.reduce<number | null>((latest, entry) => Math.max(latest ?? 0, entry.generation), null) ?? null
  const configurationGeneration = pluginConfiguration.data?.generation
  const configurationStale = configurationDraft !== undefined && configurationDraft.generation !== configurationGeneration
  const settingsConfig = configurationDraft?.value ?? (grantTarget && pluginConfiguration.isSuccess && !pluginConfiguration.isFetching
    ? JSON.stringify(pluginConfigurationWithDefaults(grantTarget.manifest.configSchema, pluginConfiguration.data.configuration), null, 2)
    : '{}')
  const settingsValue: unknown = JSON.parse(settingsConfig)
  const settingsValid = isPluginConfigurationValid(grantTarget?.manifest.configSchema, settingsValue)
  const busy = installing || readingManifest || register.isPending || approve.isPending || revoke.isPending || deploy.isPending || updateConfiguration.isPending || disable.isPending || rollback.isPending || uninstall.isPending || cleanup.isPending
  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'
  const mutationError = register.error || approve.error || revoke.error || deploy.error || updateConfiguration.error || disable.error || rollback.error || uninstall.error || cleanup.error

  const mutationText = useMemo(() => {
    if (!mutationError) return undefined
    return t('pluginLifecycle.errors.action')
  }, [mutationError, t])

  const submitRegistration = async () => {
    if (busy || !draft.manifest) return
    setReadingManifest(true)
    setRegistrationError(false)
    let manifest: Record<string, unknown>
    try {
      if (draft.manifest.size > MAX_MANIFEST_FILE_BYTES) throw new Error('Manifest file is too large')
      const parsed: unknown = JSON.parse(await draft.manifest.text())
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('invalid')
      manifest = parsed as Record<string, unknown>
    } catch {
      setRegistrationError(true)
      return
    } finally {
      setReadingManifest(false)
    }
    register.mutate({ ...draft, manifest }, {
      onSuccess: () => {
        setDraft(emptyRegistration)
        setRegistrationOpen(false)
      },
    })
  }

  const openGrant = (artifact: PluginArtifactResponse) => {
    setGenerationRefresh((current) => current + 1)
    setGrantTarget(artifact)
    setConfigurationMode(false)
    setConfigurationDraft(undefined)
    setGrants([])
    setConfig(JSON.stringify(pluginConfigurationWithDefaults(artifact.manifest.configSchema, {}), null, 2))
  }

  const openConfiguration = (deployment: PluginDeploymentResponse) => {
    setGrantTarget(deployment.artifact)
    setConfigurationMode(true)
    setConfigurationDraft(undefined)
    setGrants(deployment.grantedCapabilities.map(({ id }) => id))
    setConfig(JSON.stringify(pluginConfigurationWithDefaults(deployment.artifact.manifest.configSchema, {}), null, 2))
  }

  const submitDeployment = async () => {
    if (!grantTarget || grants.length === 0 || !generationKnown || !configurationSchemaValid) return
    const parsedConfig: unknown = (() => { try { return JSON.parse(config) } catch { return undefined } })()
    if (!parsedConfig || typeof parsedConfig !== 'object' || Array.isArray(parsedConfig)) return
    const allowed = grantableCapabilities(grantTarget).filter((item) => grants.includes(item.id))
    deploy.mutate({
      pluginId: grantTarget.pluginId,
      body: {
        artifact_digest: grantTarget.digest,
        expected_generation: currentGeneration,
        enabled: true,
        granted_capabilities: allowed.map(({ id, version }) => ({ id, version })),
        config: parsedConfig as Record<string, unknown>,
      },
    }, { onSuccess: () => setGrantTarget(undefined) })
  }

  const submitConfiguration = () => {
    if (!grantTarget || !configurationGeneration || configurationStale || !settingsValid) return
    updateConfiguration.mutate({
      pluginId: grantTarget.pluginId,
      body: {
        expected_generation: configurationGeneration,
        configuration: settingsValue as Record<string, unknown>,
      },
    }, { onSuccess: () => setGrantTarget(undefined) })
  }

  const confirmRemoval = () => {
    if (!confirmation) return
    if (confirmation.action === 'uninstall') {
      uninstall.mutate({ pluginId: confirmation.pluginId, generation: confirmation.generation }, {
        onSuccess: (result) => { setCleanupNotice(false); setUninstallNotice(result.pendingNodes); setConfirmation(undefined) },
      })
    } else {
      cleanup.mutate(confirmation.artifactId, { onSuccess: () => { setUninstallNotice(undefined); setCleanupNotice(true); setConfirmation(undefined) } })
    }
  }

  return <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
    <header className="flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="text-2xl font-semibold">{t('pluginLifecycle.title')}</h1>
        <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t('pluginLifecycle.subtitle')}</p>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button variant="bordered" onPress={() => { void artifacts.refetch(); void deployments.refetch() }}
          isDisabled={artifacts.isFetching || deployments.isFetching}>
          <RefreshCw size={16} aria-hidden="true" className={artifacts.isFetching || deployments.isFetching ? 'animate-spin' : undefined} />
          {t('pluginLifecycle.refresh')}
        </Button>
        <Button as="a" href="#plugin-install" color="primary">
          <Plus size={16} aria-hidden="true" />{t('pluginLifecycle.install.title')}
        </Button>
      </div>
    </header>

    {mutationText ? <p role="alert" className="rounded-lg border border-danger/40 bg-danger/5 px-3 py-2 text-sm text-danger">{mutationText}</p> : null}

    {uninstallNotice !== undefined ? <p role="status" className="rounded-lg border border-[var(--hairline)] p-3 text-sm text-muted-foreground">{t('pluginLifecycle.removal.accepted', { count: uninstallNotice })}</p> : null}
    {cleanupNotice ? <p role="status" className="rounded-lg border border-[var(--hairline)] p-3 text-sm text-muted-foreground">{t('pluginLifecycle.removal.cleaned')}</p> : null}
    <Card id="plugin-deployments" shadow="none" className="scroll-mt-6 rounded-xl border border-[var(--hairline)] bg-card">
      <CardHeader className="flex flex-wrap items-start justify-between gap-3 p-5">
        <div>
          <h2 className="text-base font-semibold">{t('pluginLifecycle.deployments.title')}</h2>
          <p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.deployments.description')}</p>
        </div>
        {deploymentList.length > 0 ? <Input size="sm" className="w-full sm:max-w-60"
          aria-label={t('pluginLifecycle.deployments.filter')} placeholder={t('pluginLifecycle.deployments.filter')}
          startContent={<Search size={16} aria-hidden="true" />} isClearable
          value={deploymentFilter} onValueChange={setDeploymentFilter} /> : null}
      </CardHeader>
      <CardBody className="space-y-3 p-5 pt-0">
        {deployments.isPending ? <Skeleton className="h-24 rounded-lg" /> : visibleDeployments.length ? <div className="grid w-full gap-3 sm:grid-cols-2 xl:grid-cols-3">{visibleDeployments.map((deployment) => <DeploymentCard key={deployment.pluginId} deployment={deployment} onConfigure={() => openConfiguration(deployment)} historyOpen={historyPlugin === deployment.pluginId} onToggleHistory={() => setHistoryPlugin(historyPlugin === deployment.pluginId ? undefined : deployment.pluginId)} history={historyPlugin === deployment.pluginId ? history.data ?? [] : []} historyLoading={historyPlugin === deployment.pluginId && history.isFetching} historyError={historyPlugin === deployment.pluginId && history.isError} onRetryHistory={() => void history.refetch()} onDisable={() => disable.mutate({ pluginId: deployment.pluginId, generation: deployment.generation })} onUninstall={() => setConfirmation({ action: 'uninstall', pluginId: deployment.pluginId, generation: deployment.generation })} onRollback={(targetGeneration) => rollback.mutate({ pluginId: deployment.pluginId, generation: deployment.generation, targetGeneration })} busy={busy} locale={locale} t={t} />)}</div> : deployments.isError ? null : <Empty text={t(filter ? 'pluginLifecycle.deployments.filterEmpty' : 'pluginLifecycle.deployments.empty')} />}
        {deployments.isError ? <ErrorBox onRetry={() => { if (deployments.isFetchNextPageError) void deployments.fetchNextPage(); else void deployments.refetch() }} text={t('pluginLifecycle.errors.loadDeployments')} retry={t('pluginLifecycle.refresh')} /> : null}
        {deployments.hasNextPage ? <div className="flex justify-center"><Button size="sm" variant="bordered" isDisabled={deployments.isFetching} onPress={() => void deployments.fetchNextPage()}>{t(deployments.isFetchingNextPage ? 'pluginLifecycle.loading' : 'pluginLifecycle.loadMore')}</Button></div> : null}
      </CardBody>
    </Card>

    <PluginInstallPanel disabled={busy} onInstallingChange={setInstalling} />

    <PluginRuntimePanel deployments={deploymentList} />
    <PluginTaskReconciliationPanel />
    <PluginEventResultsPanel />

    <details className="rounded-xl border border-[var(--hairline)] bg-card">
      <summary className="cursor-pointer px-5 py-4 text-sm font-medium">{t('pluginLifecycle.advanced')}</summary>
      <div className="px-4 pb-4">
    <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
      <CardHeader className="flex items-start justify-between gap-3 p-5"><div><h2 className="text-base font-semibold">{t('pluginLifecycle.artifacts.title')}</h2><p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.artifacts.description')}</p></div><Button size="sm" color="primary" isDisabled={busy} onPress={() => {
        if (registrationOpen) { setDraft(emptyRegistration); setRegistrationError(false) }
        setRegistrationOpen(!registrationOpen)
      }}><Upload size={15} />{registrationOpen ? t('pluginLifecycle.cancel') : t('pluginLifecycle.artifacts.register')}</Button></CardHeader>
      {registrationOpen ? <CardBody className="grid gap-3 border-t border-[var(--hairline)] p-5">
        <p className="text-xs text-muted-foreground">{t('pluginLifecycle.artifacts.registerHint')}</p>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('pluginLifecycle.fields.pluginId')} value={draft.plugin_id} onChange={(plugin_id) => setDraft({ ...draft, plugin_id })} />
          <Field label={t('pluginLifecycle.fields.version')} value={draft.plugin_version} onChange={(plugin_version) => setDraft({ ...draft, plugin_version })} />
          <Field label={t('pluginLifecycle.fields.packagePath')} value={draft.package_path} onChange={(package_path) => setDraft({ ...draft, package_path })} />
          <Field label={t('pluginLifecycle.fields.executablePath')} value={draft.executable_path} onChange={(executable_path) => setDraft({ ...draft, executable_path })} />
          <Field label={t('pluginLifecycle.fields.digest')} value={draft.sha256} onChange={(sha256) => setDraft({ ...draft, sha256 })} />
        </div>
        <label className="grid gap-2 text-sm font-medium">
          {t('pluginLifecycle.fields.manifest')}
          <input type="file" accept=".json,application/json" disabled={busy}
            className="block w-full rounded-lg border border-divider bg-background p-2 text-sm file:mr-3 file:rounded-md file:border-0 file:bg-surface-2 file:px-3 file:py-2 file:text-sm"
            onChange={(event) => {
              const manifest = event.target.files?.[0]
              setDraft((current) => ({ ...current, manifest }))
              setRegistrationError(false)
            }} />
        </label>
        {registrationError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.manifestFile')}</p> : null}
        <details className="text-sm"><summary className="cursor-pointer font-medium">{t('pluginLifecycle.signatureDetails')}</summary><div className="grid gap-3 pt-3"><p className="text-xs text-muted-foreground">{t('pluginLifecycle.signatureHint')}</p><Field label={t('pluginLifecycle.fields.signer')} value={draft.signer_key_id} onChange={(signer_key_id) => setDraft({ ...draft, signer_key_id })} /><Field label={t('pluginLifecycle.fields.signature')} value={draft.signature_base64} onChange={(signature_base64) => setDraft({ ...draft, signature_base64 })} /></div></details>
        <div className="flex justify-end"><Button color="primary" isDisabled={busy || !draft.plugin_id || !draft.plugin_version || !draft.package_path || !draft.executable_path || !draft.sha256 || !draft.manifest || Boolean(draft.signer_key_id.trim()) !== Boolean(draft.signature_base64.trim())} onPress={() => void submitRegistration()}>{readingManifest || register.isPending ? t('pluginLifecycle.saving') : t('pluginLifecycle.artifacts.register')}</Button></div>
      </CardBody> : null}
      <CardBody className="space-y-3 p-5 pt-0">
        {artifacts.isPending ? <Skeleton className="h-24 rounded-lg" /> : artifactList.length ? artifactList.map((artifact) => <article key={artifact.id} className="rounded-lg border border-[var(--hairline)] p-4">
          <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><strong className="break-all text-sm">{artifact.pluginId}</strong><Chip size="sm" variant="flat">{artifact.pluginVersion}</Chip><Chip size="sm" color={artifact.revocation ? 'danger' : artifact.approval ? 'success' : 'warning'} variant="flat">{artifact.revocation ? t('pluginLifecycle.status.revoked') : artifact.approval ? t('pluginLifecycle.status.approved') : t('pluginLifecycle.status.pending')}</Chip><Chip size="sm" variant="flat">{t('official' in artifact && artifact.official === true ? 'pluginLifecycle.status.official' : artifact.signerKeyId ? 'pluginLifecycle.status.signed' : 'pluginLifecycle.status.unverified')}</Chip></div><p className="mt-1 break-all font-mono text-xs text-muted-foreground">{artifact.digest}</p><p className="mt-1 text-xs text-muted-foreground">{artifact.signerKeyId ? `${artifact.signerKeyId} · ` : ''}{prettyDate(artifact.createdAt, locale)}</p><p className="mt-2 text-xs">{t('pluginLifecycle.artifacts.capabilities')}: {capabilities(artifact).map((item) => item.id).join(', ') || '—'}</p></div>
            <div className="flex flex-wrap gap-2">{!artifact.approval && !artifact.revocation ? <Button size="sm" variant="flat" color="success" isDisabled={busy} onPress={() => approve.mutate(artifact.id)}><Check size={14} />{t('pluginLifecycle.artifacts.approve')}</Button> : null}{artifact.approval && !artifact.revocation ? <Button size="sm" variant="bordered" isDisabled={busy} onPress={() => openGrant(artifact)}>{t('pluginLifecycle.deploy')}</Button> : null}{!artifact.revocation ? <Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={() => { setRevocationTarget(artifact.id); setRevocationReason('') }}>{t('pluginLifecycle.artifacts.revoke')}</Button> : null}{artifact.revocation ? <Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={() => setConfirmation({ action: 'cleanup', artifactId: artifact.id, pluginId: artifact.pluginId })}>{t('pluginLifecycle.removal.cleanup')}</Button> : null}</div>
          </div>
          {revocationTarget === artifact.id ? <div className="mt-3 flex flex-wrap items-end gap-2"><label className="min-w-56 flex-1 text-xs font-medium">{t('pluginLifecycle.artifacts.revokeReason')}<Input size="sm" maxLength={512} value={revocationReason} onChange={(event) => setRevocationReason(event.target.value)} /></label><Button size="sm" color="danger" isDisabled={busy || !revocationReason.trim()} onPress={() => revoke.mutate({ artifactId: artifact.id, reason: revocationReason.trim() }, { onSuccess: () => setRevocationTarget(undefined) })}>{t('pluginLifecycle.confirm')}</Button><Button size="sm" variant="light" onPress={() => setRevocationTarget(undefined)}>{t('pluginLifecycle.cancel')}</Button></div> : null}
        </article>) : artifacts.isError ? null : <Empty text={t('pluginLifecycle.artifacts.empty')} />}
        {artifacts.isError ? <ErrorBox onRetry={() => { if (artifacts.isFetchNextPageError) void artifacts.fetchNextPage(); else void artifacts.refetch() }} text={t('pluginLifecycle.errors.loadArtifacts')} retry={t('pluginLifecycle.refresh')} /> : null}
        {artifacts.hasNextPage ? <div className="flex justify-center"><Button size="sm" variant="bordered" isDisabled={artifacts.isFetching} onPress={() => void artifacts.fetchNextPage()}>{t(artifacts.isFetchingNextPage ? 'pluginLifecycle.loading' : 'pluginLifecycle.loadMore')}</Button></div> : null}
      </CardBody>
    </Card>
      </div>
    </details>

    <Modal isOpen={confirmation !== undefined} isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) setConfirmation(undefined) }}>
      <ModalContent>
        <ModalHeader>{t(confirmation?.action === 'cleanup' ? 'pluginLifecycle.removal.cleanupTitle' : 'pluginLifecycle.removal.uninstallTitle', { plugin: confirmation?.pluginId })}</ModalHeader>
        <ModalBody><p className="text-sm leading-6">{t(confirmation?.action === 'cleanup' ? 'pluginLifecycle.removal.cleanupHint' : 'pluginLifecycle.removal.uninstallHint')}</p>{uninstall.isError || cleanup.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.action')}</p> : null}</ModalBody>
        <ModalFooter><Button variant="light" isDisabled={busy} onPress={() => setConfirmation(undefined)}>{t('pluginLifecycle.cancel')}</Button><Button color="danger" isDisabled={busy} onPress={confirmRemoval}>{t(busy ? 'pluginLifecycle.saving' : 'pluginLifecycle.confirm')}</Button></ModalFooter>
      </ModalContent>
    </Modal>

    <Modal isOpen={grantTarget !== undefined} size="2xl" scrollBehavior="inside" isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) { setGrantTarget(undefined); setConfigurationMode(false); setConfigurationDraft(undefined) } }}>
      <ModalContent>
        <ModalHeader>{t(configurationMode ? 'pluginLifecycle.configureTitle' : 'pluginLifecycle.deployTitle', { plugin: grantTarget?.pluginId ?? '' })}</ModalHeader>
        <ModalBody>
          {!configurationMode ? <>
            <p className="text-sm text-muted-foreground">{t('pluginLifecycle.grantsHint')}</p>
            {deploymentHistory.isError ? <ErrorBox text={t('pluginLifecycle.errors.loadHistory')} retry={t('pluginLifecycle.refresh')} onRetry={() => void deploymentHistory.refetch()} /> : !generationKnown ? <p role="status" className="text-xs text-muted-foreground">{t('pluginLifecycle.checkingGeneration')}</p> : <div className="flex items-center justify-between gap-2"><p className="text-xs text-muted-foreground">{t('pluginLifecycle.currentGeneration', { generation: currentGeneration ?? '—' })}</p><Button size="sm" variant="light" isDisabled={busy} onPress={() => void deploymentHistory.refetch()}>{t('pluginLifecycle.refresh')}</Button></div>}
            <div className="space-y-2">{grantTarget ? grantableCapabilities(grantTarget).map((item) => <label key={item.id} className="flex items-start gap-2 rounded-lg border border-[var(--hairline)] p-3 text-sm"><input type="checkbox" disabled={busy} checked={grants.includes(item.id)} onChange={(event) => setGrants(event.target.checked ? [...grants, item.id] : grants.filter((id) => id !== item.id))} /><span><strong>{item.id}</strong><small className="ml-2 text-muted-foreground">v{item.version}</small></span></label>) : null}</div>
          </> : <>
            {pluginConfiguration.isPending || pluginConfiguration.isFetching ? <Skeleton className="h-24 rounded-lg" /> : null}
            {pluginConfiguration.isError ? <ErrorBox text={t('pluginLifecycle.errors.configurationLoad')} retry={t('pluginLifecycle.refresh')} onRetry={() => void pluginConfiguration.refetch()} /> : null}
            {pluginConfiguration.isSuccess && !pluginConfiguration.isFetching ? <p className="text-sm text-muted-foreground">{t('pluginLifecycle.configureHint', { generation: configurationGeneration ?? '—' })}</p> : null}
          </>}
          {grantTarget && (!configurationMode || (pluginConfiguration.isSuccess && !pluginConfiguration.isFetching)) ? <PluginConfigEditor schema={grantTarget.manifest.configSchema} value={configurationMode ? settingsConfig : config} onChange={configurationMode ? (value) => {
            if (configurationGeneration) setConfigurationDraft((previous) => ({ value, generation: previous?.generation ?? configurationGeneration }))
          } : setConfig} locale={i18n.resolvedLanguage ?? 'en'} /> : null}
          {configurationStale ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.provider.conflict')}</p> : null}
          {(!configurationMode || (pluginConfiguration.isSuccess && !pluginConfiguration.isFetching)) && !(configurationMode ? settingsValid : configurationSchemaValid) ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.configEditor.invalid')}</p> : null}
          <p className="text-xs text-muted-foreground">{t('pluginLifecycle.configHint')}</p>
          {deploy.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.action')}</p> : null}
          {updateConfiguration.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.configuration')}</p> : null}
        </ModalBody>
        <ModalFooter><Button variant="light" isDisabled={busy} onPress={() => { setGrantTarget(undefined); setConfigurationMode(false); setConfigurationDraft(undefined) }}>{t('pluginLifecycle.cancel')}</Button><Button color="primary" isDisabled={busy || (configurationMode ? !configurationGeneration || !pluginConfiguration.isSuccess || pluginConfiguration.isFetching || configurationStale || !settingsValid : !generationKnown || grants.length === 0 || !configurationSchemaValid)} onPress={() => void (configurationMode ? submitConfiguration() : submitDeployment())}>{configurationMode ? (updateConfiguration.isPending ? t('pluginLifecycle.saving') : t('pluginLifecycle.saveConfiguration')) : (deploy.isPending ? t('pluginLifecycle.saving') : t('pluginLifecycle.deploy'))}</Button></ModalFooter>
      </ModalContent>
    </Modal>
  </main>
}

function DeploymentCard({ deployment, history, historyOpen, historyLoading, historyError, onRetryHistory, onToggleHistory, onConfigure, onDisable, onUninstall, onRollback, busy, locale, t }: {
  deployment: PluginDeploymentResponse
  history: PluginDeploymentResponse[]
  historyOpen: boolean
  historyLoading: boolean
  historyError: boolean
  onRetryHistory: () => void
  onToggleHistory: () => void
  onConfigure: () => void
  onDisable: () => void
  onUninstall: () => void
  onRollback: (targetGeneration: number) => void
  busy: boolean
  locale: string
  t: TFunction
}) {
  const [providerEditorOpen, setProviderEditorOpen] = useState(false)
  const canConfigureProvider = deployment.enabled && deployment.grantedCapabilities.some(({ id }) => id === 'provider.lifecycle')
  const uiCards = pluginUiCards(deployment, locale)
  const pages = pluginPages(deployment, locale)
  const uiViewsEnabled = deployment.enabled && Boolean(deployment.artifact.approval) && !deployment.artifact.revocation
    && hasRenderablePluginUiViews(deployment.artifact.manifest, 'plugin_management', locale)
  const manifest = deployment.artifact.manifest
  const displayName = pluginUiText(manifest.displayName, locale) ?? deployment.pluginId
  const description = pluginDescription(deployment, uiCards, locale, t('pluginLifecycle.deployments.noDescription'))
  const trust = t('official' in deployment.artifact && deployment.artifact.official === true
    ? 'pluginLifecycle.status.official'
    : deployment.artifact.signerKeyId
      ? 'pluginLifecycle.status.signed'
      : 'pluginLifecycle.status.unverified')
  return <article data-plugin-id={deployment.pluginId} className="flex h-full min-w-0 flex-col rounded-xl border border-[var(--hairline)] bg-card p-4 sm:p-5">
    <div className="flex min-w-0 items-start justify-between gap-3">
      <div className="min-w-0">
        <h3 className="break-words text-base font-semibold">{displayName}</h3>
        <p className="mt-1 min-h-12 whitespace-pre-wrap break-words text-sm leading-5 text-muted-foreground line-clamp-3">{description}</p>
        <p className="mt-2 break-all text-xs text-muted-foreground">{deployment.pluginId}</p>
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1">
        <Chip size="sm" color={deployment.enabled ? 'success' : 'default'} variant="flat">{deployment.enabled ? t('pluginLifecycle.status.enabled') : t('pluginLifecycle.status.disabled')}</Chip>
        <Chip size="sm" variant="flat">{trust}</Chip>
      </div>
    </div>
    <p className="mt-3 text-xs text-muted-foreground">v{deployment.artifact.pluginVersion} · g{deployment.generation}</p>
    <Button className="mt-4 w-full" color="primary" isDisabled={busy} onPress={onConfigure}>{t('pluginLifecycle.configure')}</Button>
    <details className="mt-3 border-t border-divider pt-3">
      <summary className="cursor-pointer text-sm font-medium">{t('pluginLifecycle.deployments.more')}</summary>
      <div className="mt-3 grid gap-3">
        <div className="flex flex-wrap gap-2">
          {deployment.enabled ? <Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={onDisable}>{t('pluginLifecycle.disable')}</Button> : null}
          <Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={onUninstall}>{t('pluginLifecycle.removal.uninstall')}</Button>
          <Button size="sm" variant="bordered" isDisabled={historyLoading} onPress={onToggleHistory}><Clock3 size={14} />{t('pluginLifecycle.deployments.history')}</Button>
          {canConfigureProvider ? <Button size="sm" variant="bordered" isDisabled={busy} onPress={() => setProviderEditorOpen(true)}>{t('pluginLifecycle.provider.title')}</Button> : null}
        </div>
        <p className="break-words text-xs text-muted-foreground">{t('pluginLifecycle.deployments.grants')}: {deployment.grantedCapabilities.map(({ id, version }) => `${id}@${version}`).join(', ') || '—'}</p>
        {canConfigureProvider ? <p className="text-xs text-muted-foreground">{t('pluginLifecycle.provider.bound', { count: deployment.providerSelection?.channelIds.length ?? 0 })}</p> : null}
        {uiCards.length || uiViewsEnabled || pages.length ? <section aria-label={t('pluginLifecycle.ui.section')} className="grid gap-2 border-t border-[var(--hairline)] pt-3">
          <p className="text-xs text-muted-foreground">{t('pluginLifecycle.ui.scope')}</p>
          {uiCards.map((card) => <div key={`${deployment.pluginId}:${card.id}`} className="rounded-lg border border-[var(--hairline)] bg-content1/40 p-3" data-plugin-ui-card={card.id}><h4 className="text-sm font-medium">{card.title}</h4>{card.description ? <p className="mt-1 whitespace-pre-wrap text-sm text-muted-foreground">{card.description}</p> : null}</div>)}
          {uiViewsEnabled ? <PluginUiViewsRenderer manifest={deployment.artifact.manifest} slot="plugin_management" locale={locale} /> : null}
          {pages.length ? <div className="flex flex-wrap gap-2" aria-label={t('pluginLifecycle.pages.title')}>{pages.map((page) => <Button key={page.id} as={Link} to={`/console/plugin-pages/${encodeURIComponent(deployment.pluginId)}/${encodeURIComponent(page.id)}`} size="sm" variant="bordered">{page.title}</Button>)}</div> : null}
        </section> : null}
        <PluginCapabilityConsole deployment={deployment} />
        {historyOpen ? <div className="border-t border-[var(--hairline)] pt-3">{historyLoading ? <Skeleton className="h-10 rounded" /> : historyError ? <ErrorBox text={t('pluginLifecycle.errors.loadHistory')} retry={t('pluginLifecycle.refresh')} onRetry={onRetryHistory} /> : <div className="space-y-2">{history.map((entry) => <div key={entry.generation} className="flex flex-wrap items-center justify-between gap-2 text-xs"><span>g{entry.generation} · {entry.artifact.pluginVersion} · {entry.enabled ? t('pluginLifecycle.status.enabled') : t('pluginLifecycle.status.disabled')} · {prettyDate(entry.updatedAt, locale)}</span>{entry.generation < deployment.generation && entry.enabled ? <Button size="sm" variant="flat" isDisabled={busy} onPress={() => onRollback(entry.generation)}>{t('pluginLifecycle.rollback')}</Button> : null}</div>)}</div>}</div> : null}
      </div>
    </details>
    {canConfigureProvider && providerEditorOpen ? <PluginProviderEditor deployment={deployment} onClose={() => setProviderEditorOpen(false)} /> : null}
  </article>
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return <Input label={label} size="sm" value={value} onChange={(event) => onChange(event.target.value)} />
}

function ErrorBox({ text, retry, onRetry }: { text: string; retry: string; onRetry: () => void }) {
  return <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-danger/30 bg-danger/5 p-4 text-sm"><span>{text}</span><Button size="sm" variant="bordered" onPress={onRetry}>{retry}</Button></div>
}

function Empty({ text }: { text: string }) {
  return <p className="rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center text-sm text-muted-foreground">{text}</p>
}
