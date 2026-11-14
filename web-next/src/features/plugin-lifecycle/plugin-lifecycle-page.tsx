import { Button, Card, CardBody, CardHeader, Chip, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Skeleton, Textarea } from '@heroui/react'
import { Box, Check, Clock3, RefreshCw, ShieldAlert, Upload } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TFunction } from 'i18next'

import type { PluginArtifactResponse, PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import { PluginInstallPanel } from './plugin-install-panel'
import { PluginRuntimePanel } from './plugin-runtime-panel'
import {
  useApprovePluginArtifact,
  useDeployPluginArtifact,
  useDisablePluginDeployment,
  usePluginArtifacts,
  usePluginDeploymentHistory,
  usePluginDeployments,
  useRegisterPluginArtifact,
  useRevokePluginArtifact,
  useRollbackPluginDeployment,
  useUninstallPlugin,
  useCleanupPluginArtifact,
} from './plugin-lifecycle-api'

type ArtifactRegistrationDraft = {
  plugin_id: string
  plugin_version: string
  package_path: string
  executable_path: string
  sha256: string
  signer_key_id: string
  signature_base64: string
  manifest: string
}

const emptyRegistration: ArtifactRegistrationDraft = {
  plugin_id: '', plugin_version: '', package_path: '', executable_path: '', sha256: '',
  signer_key_id: '', signature_base64: '', manifest: '',
}

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
    item.access === 'read'
      && item.sideEffect === 'none'
      && item.id !== 'external.account.profile.read'
      && item.id !== 'external.account.balance.read',
  )
}

function configurationSchema(artifact: PluginArtifactResponse) {
  const schema = artifact.manifest.configSchema
  if (!schema || typeof schema !== 'object' || Array.isArray(schema)) return '{}'
  return JSON.stringify(schema, null, 2)
}

function prettyDate(value: number, locale: string) {
  return new Date(value * 1_000).toLocaleString(locale)
}

export function PluginLifecyclePage() {
  const { t, i18n } = useTranslation()
  const artifacts = usePluginArtifacts()
  const deployments = usePluginDeployments()
  const register = useRegisterPluginArtifact()
  const approve = useApprovePluginArtifact()
  const revoke = useRevokePluginArtifact()
  const deploy = useDeployPluginArtifact()
  const disable = useDisablePluginDeployment()
  const rollback = useRollbackPluginDeployment()
  const uninstall = useUninstallPlugin()
  const cleanup = useCleanupPluginArtifact()
  const [draft, setDraft] = useState(emptyRegistration)
  const [registrationOpen, setRegistrationOpen] = useState(false)
  const [registrationError, setRegistrationError] = useState(false)
  const [grantTarget, setGrantTarget] = useState<PluginArtifactResponse>()
  const [grants, setGrants] = useState<string[]>([])
  const [config, setConfig] = useState('{}')
  const [revocationTarget, setRevocationTarget] = useState<number>()
  const [revocationReason, setRevocationReason] = useState('')
  const [historyPlugin, setHistoryPlugin] = useState<string>()
  const [generationRefresh, setGenerationRefresh] = useState(0)
  const [installing, setInstalling] = useState(false)
  const [confirmation, setConfirmation] = useState<
    { action: 'uninstall'; pluginId: string; generation: number }
    | { action: 'cleanup'; artifactId: number; pluginId: string }
  >()
  const [uninstallNotice, setUninstallNotice] = useState<number>()
  const [cleanupNotice, setCleanupNotice] = useState(false)
  const history = usePluginDeploymentHistory(historyPlugin)
  const deploymentHistory = usePluginDeploymentHistory(grantTarget?.pluginId, generationRefresh)
  const artifactList = artifacts.data?.pages.flatMap((page) => page.artifacts) ?? []
  const deploymentList = deployments.data?.pages.flatMap((page) => page.deployments) ?? []
  const generationKnown = deploymentHistory.isSuccess && !deploymentHistory.isFetching
  const currentGeneration = deploymentHistory.data?.reduce<number | null>((latest, entry) => Math.max(latest ?? 0, entry.generation), null) ?? null
  const busy = installing || register.isPending || approve.isPending || revoke.isPending || deploy.isPending || disable.isPending || rollback.isPending || uninstall.isPending || cleanup.isPending
  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'
  const mutationError = register.error || approve.error || revoke.error || deploy.error || disable.error || rollback.error || uninstall.error || cleanup.error

  const mutationText = useMemo(() => {
    if (!mutationError) return undefined
    return t('pluginLifecycle.errors.action')
  }, [mutationError, t])

  const submitRegistration = async () => {
    let manifest: Record<string, unknown>
    try {
      const parsed: unknown = JSON.parse(draft.manifest)
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('invalid')
      manifest = parsed as Record<string, unknown>
    } catch {
      setRegistrationError(true)
      return
    }
    setRegistrationError(false)
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
    setGrants([])
    setConfig('{}')
  }

  const submitDeployment = async () => {
    if (!grantTarget || grants.length === 0 || !generationKnown) return
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
    <header className="flex flex-wrap items-start justify-between gap-3">
      <div><p className="flex items-center gap-2 text-xs font-semibold tracking-wide text-muted-foreground"><Box size={15} />{t('pluginLifecycle.eyebrow')}</p><h1 className="mt-1 text-2xl font-semibold">{t('pluginLifecycle.title')}</h1><p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t('pluginLifecycle.subtitle')}</p></div>
      <Button size="sm" variant="bordered" onPress={() => { void artifacts.refetch(); void deployments.refetch() }} isDisabled={artifacts.isFetching || deployments.isFetching}><RefreshCw size={15} className={(artifacts.isFetching || deployments.isFetching) ? 'animate-spin' : undefined} />{t('pluginLifecycle.refresh')}</Button>
    </header>

    <div className="flex items-start gap-3 rounded-xl border border-warning/40 bg-warning/10 p-4 text-sm"><ShieldAlert className="mt-0.5 shrink-0 text-warning" size={18} /><p>{t('pluginLifecycle.trustWarning')}</p></div>
    {mutationText ? <p role="alert" className="rounded-lg border border-danger/40 bg-danger/5 px-3 py-2 text-sm text-danger">{mutationText}</p> : null}
    {registrationError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.manifestJson')}</p> : null}

    <PluginInstallPanel disabled={busy} onInstallingChange={setInstalling} />
    {uninstallNotice !== undefined ? <p role="status" className="rounded-lg border border-[var(--hairline)] p-3 text-sm text-muted-foreground">{t('pluginLifecycle.removal.accepted', { count: uninstallNotice })}</p> : null}
    {cleanupNotice ? <p role="status" className="rounded-lg border border-[var(--hairline)] p-3 text-sm text-muted-foreground">{t('pluginLifecycle.removal.cleaned')}</p> : null}
    <PluginRuntimePanel deployments={deploymentList} />

    <details className="rounded-xl border border-[var(--hairline)] bg-card">
      <summary className="cursor-pointer px-5 py-4 text-sm font-medium">{t('pluginLifecycle.advanced')}</summary>
      <div className="px-4 pb-4">
    <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
      <CardHeader className="flex items-start justify-between gap-3 p-5"><div><h2 className="text-base font-semibold">{t('pluginLifecycle.artifacts.title')}</h2><p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.artifacts.description')}</p></div><Button size="sm" color="primary" onPress={() => setRegistrationOpen(!registrationOpen)}><Upload size={15} />{registrationOpen ? t('pluginLifecycle.cancel') : t('pluginLifecycle.artifacts.register')}</Button></CardHeader>
      {registrationOpen ? <CardBody className="grid gap-3 border-t border-[var(--hairline)] p-5">
        <p className="text-xs text-muted-foreground">{t('pluginLifecycle.artifacts.registerHint')}</p>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t('pluginLifecycle.fields.pluginId')} value={draft.plugin_id} onChange={(plugin_id) => setDraft({ ...draft, plugin_id })} />
          <Field label={t('pluginLifecycle.fields.version')} value={draft.plugin_version} onChange={(plugin_version) => setDraft({ ...draft, plugin_version })} />
          <Field label={t('pluginLifecycle.fields.packagePath')} value={draft.package_path} onChange={(package_path) => setDraft({ ...draft, package_path })} />
          <Field label={t('pluginLifecycle.fields.executablePath')} value={draft.executable_path} onChange={(executable_path) => setDraft({ ...draft, executable_path })} />
          <Field label={t('pluginLifecycle.fields.digest')} value={draft.sha256} onChange={(sha256) => setDraft({ ...draft, sha256 })} />
        </div>
        <Textarea label={t('pluginLifecycle.fields.manifest')} minRows={7} value={draft.manifest} onValueChange={(manifest) => { setDraft({ ...draft, manifest }); setRegistrationError(false) }} classNames={{ input: 'font-mono text-xs' }} />
        <details className="text-sm"><summary className="cursor-pointer font-medium">{t('pluginLifecycle.signatureDetails')}</summary><div className="grid gap-3 pt-3"><p className="text-xs text-muted-foreground">{t('pluginLifecycle.signatureHint')}</p><Field label={t('pluginLifecycle.fields.signer')} value={draft.signer_key_id} onChange={(signer_key_id) => setDraft({ ...draft, signer_key_id })} /><Textarea label={t('pluginLifecycle.fields.signature')} minRows={2} value={draft.signature_base64} onValueChange={(signature_base64) => setDraft({ ...draft, signature_base64 })} classNames={{ input: 'font-mono text-xs' }} /></div></details>
        <div className="flex justify-end"><Button color="primary" isDisabled={busy || !draft.plugin_id || !draft.plugin_version || !draft.package_path || !draft.executable_path || !draft.sha256 || !validObjectJson(draft.manifest) || Boolean(draft.signer_key_id.trim()) !== Boolean(draft.signature_base64.trim())} onPress={() => void submitRegistration()}>{register.isPending ? t('pluginLifecycle.saving') : t('pluginLifecycle.artifacts.register')}</Button></div>
      </CardBody> : null}
      <CardBody className="space-y-3 p-5 pt-0">
        {artifacts.isPending ? <Skeleton className="h-24 rounded-lg" /> : artifactList.length ? artifactList.map((artifact) => <article key={artifact.id} className="rounded-lg border border-[var(--hairline)] p-4">
          <div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><strong className="text-sm">{artifact.pluginId}</strong><Chip size="sm" variant="flat">{artifact.pluginVersion}</Chip><Chip size="sm" color={artifact.revocation ? 'danger' : artifact.approval ? 'success' : 'warning'} variant="flat">{artifact.revocation ? t('pluginLifecycle.status.revoked') : artifact.approval ? t('pluginLifecycle.status.approved') : t('pluginLifecycle.status.pending')}</Chip><Chip size="sm" variant="flat">{t('official' in artifact && artifact.official === true ? 'pluginLifecycle.status.official' : artifact.signerKeyId ? 'pluginLifecycle.status.signed' : 'pluginLifecycle.status.unverified')}</Chip></div><p className="mt-1 break-all font-mono text-xs text-muted-foreground">{artifact.digest}</p><p className="mt-1 text-xs text-muted-foreground">{artifact.signerKeyId ? `${artifact.signerKeyId} · ` : ''}{prettyDate(artifact.createdAt, locale)}</p><p className="mt-2 text-xs">{t('pluginLifecycle.artifacts.capabilities')}: {capabilities(artifact).map((item) => item.id).join(', ') || '—'}</p></div>
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

    <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
      <CardHeader className="p-5"><div><h2 className="text-base font-semibold">{t('pluginLifecycle.deployments.title')}</h2><p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.deployments.description')}</p></div></CardHeader>
      <CardBody className="space-y-3 p-5 pt-0">
        {deployments.isPending ? <Skeleton className="h-24 rounded-lg" /> : deploymentList.length ? deploymentList.map((deployment) => <DeploymentCard key={deployment.pluginId} deployment={deployment} historyOpen={historyPlugin === deployment.pluginId} onToggleHistory={() => setHistoryPlugin(historyPlugin === deployment.pluginId ? undefined : deployment.pluginId)} history={historyPlugin === deployment.pluginId ? history.data ?? [] : []} historyLoading={historyPlugin === deployment.pluginId && history.isFetching} historyError={historyPlugin === deployment.pluginId && history.isError} onRetryHistory={() => void history.refetch()} onDisable={() => disable.mutate({ pluginId: deployment.pluginId, generation: deployment.generation })} onUninstall={() => setConfirmation({ action: 'uninstall', pluginId: deployment.pluginId, generation: deployment.generation })} onRollback={(targetGeneration) => rollback.mutate({ pluginId: deployment.pluginId, generation: deployment.generation, targetGeneration })} busy={busy} locale={locale} t={t} />) : deployments.isError ? null : <Empty text={t('pluginLifecycle.deployments.empty')} />}
        {deployments.isError ? <ErrorBox onRetry={() => { if (deployments.isFetchNextPageError) void deployments.fetchNextPage(); else void deployments.refetch() }} text={t('pluginLifecycle.errors.loadDeployments')} retry={t('pluginLifecycle.refresh')} /> : null}
        {deployments.hasNextPage ? <div className="flex justify-center"><Button size="sm" variant="bordered" isDisabled={deployments.isFetching} onPress={() => void deployments.fetchNextPage()}>{t(deployments.isFetchingNextPage ? 'pluginLifecycle.loading' : 'pluginLifecycle.loadMore')}</Button></div> : null}
      </CardBody>
    </Card>

    <Modal isOpen={confirmation !== undefined} isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) setConfirmation(undefined) }}>
      <ModalContent>
        <ModalHeader>{t(confirmation?.action === 'cleanup' ? 'pluginLifecycle.removal.cleanupTitle' : 'pluginLifecycle.removal.uninstallTitle', { plugin: confirmation?.pluginId })}</ModalHeader>
        <ModalBody><p className="text-sm leading-6">{t(confirmation?.action === 'cleanup' ? 'pluginLifecycle.removal.cleanupHint' : 'pluginLifecycle.removal.uninstallHint')}</p>{uninstall.isError || cleanup.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.action')}</p> : null}</ModalBody>
        <ModalFooter><Button variant="light" isDisabled={busy} onPress={() => setConfirmation(undefined)}>{t('pluginLifecycle.cancel')}</Button><Button color="danger" isDisabled={busy} onPress={confirmRemoval}>{t(busy ? 'pluginLifecycle.saving' : 'pluginLifecycle.confirm')}</Button></ModalFooter>
      </ModalContent>
    </Modal>

    <Modal isOpen={grantTarget !== undefined} size="2xl" scrollBehavior="inside" isDismissable={!busy} isKeyboardDismissDisabled={busy} onOpenChange={(open) => { if (!open && !busy) setGrantTarget(undefined) }}>
      <ModalContent>
        <ModalHeader>{t('pluginLifecycle.deployTitle', { plugin: grantTarget?.pluginId ?? '' })}</ModalHeader>
        <ModalBody>
          <p className="text-sm text-muted-foreground">{t('pluginLifecycle.grantsHint')}</p>
          {deploymentHistory.isError ? <ErrorBox text={t('pluginLifecycle.errors.loadHistory')} retry={t('pluginLifecycle.refresh')} onRetry={() => void deploymentHistory.refetch()} /> : !generationKnown ? <p role="status" className="text-xs text-muted-foreground">{t('pluginLifecycle.checkingGeneration')}</p> : <div className="flex items-center justify-between gap-2"><p className="text-xs text-muted-foreground">{t('pluginLifecycle.currentGeneration', { generation: currentGeneration ?? '—' })}</p><Button size="sm" variant="light" isDisabled={busy} onPress={() => void deploymentHistory.refetch()}>{t('pluginLifecycle.refresh')}</Button></div>}
          <div className="space-y-2">{grantTarget ? grantableCapabilities(grantTarget).map((item) => <label key={item.id} className="flex items-start gap-2 rounded-lg border border-[var(--hairline)] p-3 text-sm"><input type="checkbox" disabled={busy} checked={grants.includes(item.id)} onChange={(event) => setGrants(event.target.checked ? [...grants, item.id] : grants.filter((id) => id !== item.id))} /><span><strong>{item.id}</strong><small className="ml-2 text-muted-foreground">v{item.version}</small></span></label>) : null}</div>
          <Textarea label={t('pluginLifecycle.fields.config')} description={t('pluginLifecycle.configHint')} minRows={5} value={config} onValueChange={setConfig} isDisabled={busy} classNames={{ input: 'font-mono text-xs' }} />
          {grantTarget ? <details className="text-xs"><summary className="cursor-pointer font-medium">{t('pluginLifecycle.fields.configSchema')}</summary><pre className="mt-2 overflow-auto whitespace-pre-wrap rounded-lg bg-surface-2 p-3 font-mono">{configurationSchema(grantTarget)}</pre></details> : null}
          {deploy.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.errors.action')}</p> : null}
        </ModalBody>
        <ModalFooter><Button variant="light" isDisabled={busy} onPress={() => setGrantTarget(undefined)}>{t('pluginLifecycle.cancel')}</Button><Button color="primary" isDisabled={busy || !generationKnown || grants.length === 0 || !validObjectJson(config)} onPress={() => void submitDeployment()}>{deploy.isPending ? t('pluginLifecycle.saving') : t('pluginLifecycle.deploy')}</Button></ModalFooter>
      </ModalContent>
    </Modal>
  </main>
}

function DeploymentCard({ deployment, history, historyOpen, historyLoading, historyError, onRetryHistory, onToggleHistory, onDisable, onUninstall, onRollback, busy, locale, t }: {
  deployment: PluginDeploymentResponse
  history: PluginDeploymentResponse[]
  historyOpen: boolean
  historyLoading: boolean
  historyError: boolean
  onRetryHistory: () => void
  onToggleHistory: () => void
  onDisable: () => void
  onUninstall: () => void
  onRollback: (targetGeneration: number) => void
  busy: boolean
  locale: string
  t: TFunction
}) {
  return <article className="rounded-lg border border-[var(--hairline)] p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div><div className="flex flex-wrap items-center gap-2"><strong className="text-sm">{deployment.pluginId}</strong><Chip size="sm" variant="flat">g{deployment.generation}</Chip><Chip size="sm" color={deployment.enabled ? 'success' : 'default'} variant="flat">{deployment.enabled ? t('pluginLifecycle.status.enabled') : t('pluginLifecycle.status.disabled')}</Chip><Chip size="sm" variant="flat">{t('official' in deployment.artifact && deployment.artifact.official === true ? 'pluginLifecycle.status.official' : deployment.artifact.signerKeyId ? 'pluginLifecycle.status.signed' : 'pluginLifecycle.status.unverified')}</Chip></div><p className="mt-1 text-xs text-muted-foreground">{deployment.artifact.pluginVersion} · {deployment.artifact.digest.slice(0, 16)}… · {prettyDate(deployment.updatedAt, locale)}</p><p className="mt-2 text-xs">{t('pluginLifecycle.deployments.grants')}: {deployment.grantedCapabilities.map(({ id, version }) => `${id}@${version}`).join(', ') || '—'}</p></div><div className="flex flex-wrap gap-2">{deployment.enabled ? <Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={onDisable}>{t('pluginLifecycle.disable')}</Button> : null}<Button size="sm" variant="bordered" color="danger" isDisabled={busy} onPress={onUninstall}>{t('pluginLifecycle.removal.uninstall')}</Button><Button size="sm" variant="light" isDisabled={historyLoading} onPress={onToggleHistory}><Clock3 size={14} />{t('pluginLifecycle.deployments.history')}</Button></div></div>
    {historyOpen ? <div className="mt-3 border-t border-[var(--hairline)] pt-3">{historyLoading ? <Skeleton className="h-10 rounded" /> : historyError ? <ErrorBox text={t('pluginLifecycle.errors.loadHistory')} retry={t('pluginLifecycle.refresh')} onRetry={onRetryHistory} /> : <div className="space-y-2">{history.map((entry) => <div key={entry.generation} className="flex flex-wrap items-center justify-between gap-2 text-xs"><span>g{entry.generation} · {entry.artifact.pluginVersion} · {entry.enabled ? t('pluginLifecycle.status.enabled') : t('pluginLifecycle.status.disabled')} · {prettyDate(entry.updatedAt, locale)}</span>{entry.generation < deployment.generation && entry.enabled ? <Button size="sm" variant="flat" isDisabled={busy} onPress={() => onRollback(entry.generation)}>{t('pluginLifecycle.rollback')}</Button> : null}</div>)}</div>}</div> : null}
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

function validObjectJson(value: string) {
  try {
    const parsed: unknown = JSON.parse(value)
    return Boolean(parsed && typeof parsed === 'object' && !Array.isArray(parsed))
  } catch {
    return false
  }
}
