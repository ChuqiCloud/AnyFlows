import { Button, Card, CardBody, CardHeader, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Skeleton, Textarea } from '@heroui/react'
import { ArrowLeft, Play, ShieldCheck } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link, useParams } from 'react-router-dom'

import type { PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import {
  useConfirmPluginCapabilityAction,
  usePluginCustomCapability,
  usePluginCapabilityActionProposal,
  usePluginDeployments,
} from './plugin-lifecycle-api'
import { PluginConfigEditor } from './plugin-config-editor'
import { pluginPages, type PluginPageDefinition } from './plugin-pages'
import {
  parsePluginUiBlocks,
  PluginUiBlocksRenderer,
} from './plugin-ui-views'

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}

function deploymentForPlugin(deployments: PluginDeploymentResponse[], pluginId: string) {
  return deployments.find((deployment) => deployment.pluginId === pluginId)
}

export function PluginPage() {
  const { pluginId = '', pageId = '' } = useParams()
  const { t, i18n } = useTranslation()
  const locale = i18n.resolvedLanguage ?? 'en'
  const deployments = usePluginDeployments()
  const invocation = usePluginCustomCapability()
  const [input, setInput] = useState('{}')
  const [activeCallKey, setActiveCallKey] = useState<string>()
  const [completedCallKey, setCompletedCallKey] = useState<string>()
  const deploymentList = useMemo(
    () => deployments.data?.pages.flatMap((page) => page.deployments) ?? [],
    [deployments.data],
  )
  const deployment = deploymentForPlugin(deploymentList, pluginId)
  const page = useMemo(
    () => deployment ? pluginPages(deployment, locale).find((item) => item.id === pageId) : undefined,
    [deployment, locale, pageId],
  )
  const inputSchema = record(page?.inputSchema)
  const inputValue = useMemo(() => {
    try {
      return { valid: true as const, value: JSON.parse(input) as unknown }
    } catch {
      return { valid: false as const }
    }
  }, [input])
  const callKey = `${pluginId}:${deployment?.generation ?? ''}:${pageId}:${input}`
  const output = completedCallKey === callKey ? invocation.data?.output : undefined
  const blocks = output === undefined ? undefined : parsePluginUiBlocks(output, locale)
  const callError = activeCallKey === callKey && invocation.isError
  const updateInput = (next: string) => {
    setInput(next)
    setActiveCallKey(undefined)
    setCompletedCallKey(undefined)
    invocation.reset()
  }

  useEffect(() => {
    setInput(JSON.stringify(page?.input ?? {}, null, 2))
    setActiveCallKey(undefined)
    setCompletedCallKey(undefined)
    invocation.reset()
  }, [deployment?.generation, invocation.reset, page?.id, page?.input, pluginId])

  useEffect(() => {
    if (pluginId && !deployment && !deployments.isError && deployments.hasNextPage && !deployments.isFetchingNextPage) {
      void deployments.fetchNextPage()
    }
  }, [deployment, deployments.fetchNextPage, deployments.hasNextPage, deployments.isError, deployments.isFetchingNextPage, pluginId])

  const run = () => {
    if (!deployment || !page || !inputValue.valid) return
    const currentCallKey = `${pluginId}:${deployment.generation}:${pageId}:${input}`
    setActiveCallKey(currentCallKey)
    setCompletedCallKey(undefined)
    invocation.reset()
    invocation.mutate({
      pluginId: deployment.pluginId,
      capabilityId: page.capabilityId,
      capabilityVersion: page.capabilityVersion,
      input: inputValue.value,
    }, { onSuccess: () => setCompletedCallKey(currentCallKey) })
  }

  return <main className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-4 sm:p-6">
    <Button as={Link} to="/console/plugin-lifecycle" className="w-fit" variant="light" size="sm"><ArrowLeft size={14} />{t('pluginLifecycle.pages.back')}</Button>
    {deployments.isPending || deployments.isFetchingNextPage ? <Skeleton className="h-40 rounded-xl" /> : null}
    {deployment && page ? <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
      <CardHeader className="flex items-start justify-between gap-3 p-5">
        <div>
          <p className="text-xs font-semibold tracking-wide text-muted-foreground">{deployment.pluginId}</p>
          <h1 className="mt-1 text-2xl font-semibold">{page.title}</h1>
          {page.description ? <p className="mt-2 max-w-3xl text-sm text-muted-foreground">{page.description}</p> : null}
        </div>
        <Button color="primary" isDisabled={invocation.isPending || !inputValue.valid} onPress={run}>
          <Play size={15} />{t(invocation.isPending ? 'pluginLifecycle.pages.running' : 'pluginLifecycle.pages.refresh')}
        </Button>
      </CardHeader>
      <CardBody className="space-y-3 p-5 pt-0">
        {inputSchema && (inputSchema.type === 'object' || record(inputSchema.properties))
          ? <PluginConfigEditor schema={inputSchema} value={input} onChange={updateInput} locale={locale} />
          : <Textarea aria-label={t('pluginLifecycle.fields.config')} minRows={5} value={input} onValueChange={updateInput} classNames={{ input: 'font-mono text-xs' }} />}
        {!inputValue.valid ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.pages.invalidInput')}</p> : null}
        {callError ? <p role="alert" className="rounded-lg border border-danger/30 bg-danger/5 p-3 text-sm text-danger">{t('pluginLifecycle.pages.invokeError')}</p> : null}
        {blocks ? <PluginUiBlocksRenderer blocks={blocks} /> : output === undefined && !callError ? <p className="text-sm text-muted-foreground">{t('pluginLifecycle.pages.empty')}</p> : null}
        {output !== undefined && !blocks ? <PluginJsonOutput value={output} tooLarge={t('pluginLifecycle.pages.outputTooLarge')} /> : null}
        {page.actions.length ? <PluginPageActions deployment={deployment} page={page} locale={locale} /> : null}
      </CardBody>
    </Card> : !deployments.isPending && !deployments.isFetchingNextPage ? <p role="alert" className="rounded-lg border border-warning/30 bg-warning/5 p-4 text-sm">{t('pluginLifecycle.pages.unavailable')}</p> : null}
  </main>
}

function PluginJsonOutput({ value, tooLarge }: { value: unknown; tooLarge: string }) {
  const json = JSON.stringify(value, null, 2)
  if (json === undefined || new TextEncoder().encode(json).length > 32 * 1024) {
    return <p className="text-sm text-muted-foreground">{tooLarge}</p>
  }
  return <pre className="max-h-[32rem] overflow-auto whitespace-pre-wrap break-all rounded-lg bg-content1/50 p-3 font-mono text-xs">{json}</pre>
}

function PluginPageActions({ deployment, page, locale }: { deployment: PluginDeploymentResponse; page: PluginPageDefinition; locale: string }) {
  const { t } = useTranslation()
  const proposal = usePluginCapabilityActionProposal()
  const confirm = useConfirmPluginCapabilityAction()
  const [actionId, setActionId] = useState(page.actions[0]?.id ?? '')
  const action = page.actions.find((candidate) => candidate.id === actionId) ?? page.actions[0]
  const [input, setInput] = useState(() => JSON.stringify(action?.input ?? {}, null, 2))
  const [proposalKey, setProposalKey] = useState<string>()
  const [confirmationKey, setConfirmationKey] = useState<string>()
  const [proposalStatus, setProposalStatus] = useState<string>()
  const [resultStatus, setResultStatus] = useState<string>()
  const [modalOpen, setModalOpen] = useState(false)
  const parsed = useMemo(() => {
    try { return { valid: true as const, value: JSON.parse(input) as unknown } } catch { return { valid: false as const } }
  }, [input])
  const inputSchema = record(action?.inputSchema)
  const resetAction = (nextId: string) => {
    const next = page.actions.find((candidate) => candidate.id === nextId)
    setActionId(nextId)
    setInput(JSON.stringify(next?.input ?? {}, null, 2))
    setProposalKey(undefined)
    setConfirmationKey(undefined)
    setProposalStatus(undefined)
    setResultStatus(undefined)
    proposal.reset()
    confirm.reset()
  }
  useEffect(() => {
    setInput(JSON.stringify(action?.input ?? {}, null, 2))
  }, [action?.id, action?.input])
  const updateInput = (next: string) => {
    setInput(next)
    setProposalKey(undefined)
    setResultStatus(undefined)
    proposal.reset()
    confirm.reset()
  }
  const createProposal = () => {
    if (!action || !parsed.valid || proposal.isPending || confirm.isPending) return
    proposal.mutate({
      pluginId: deployment.pluginId,
      capabilityId: action.capabilityId,
      capabilityVersion: action.capabilityVersion,
      idempotencyKey: crypto.randomUUID(),
      input: parsed.value,
    }, {
      onSuccess: (result) => {
        setProposalKey(result.proposalKey)
        setProposalStatus(result.status)
        setConfirmationKey(crypto.randomUUID())
        setModalOpen(true)
      },
    })
  }
  const executeProposal = () => {
    if (!proposalKey || !confirmationKey || confirm.isPending) return
    confirm.mutate({ proposalKey, confirmationKey, input: parsed.value }, {
      onSuccess: (result) => {
        setResultStatus(result.outcomeCode ? `${result.status}:${result.outcomeCode}` : result.status)
        setModalOpen(false)
      },
    })
  }
  const finished = Boolean(resultStatus) || confirm.isPending
  if (!action) return null
  return <section className="mt-4 space-y-3 rounded-lg border border-warning/30 bg-warning/5 p-4" aria-label={t('pluginLifecycle.pages.actions.title')}>
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div><h2 className="text-sm font-semibold">{t('pluginLifecycle.pages.actions.title')}</h2><p className="mt-1 text-xs text-muted-foreground">{t('pluginLifecycle.pages.actions.description')}</p></div>
      {page.actions.length > 1 ? <select aria-label={t('pluginLifecycle.pages.actions.select')} className="h-9 max-w-full rounded-lg border border-[var(--hairline)] bg-background px-2 text-sm" value={action.id} onChange={(event) => resetAction(event.target.value)}>{page.actions.map((candidate) => <option key={candidate.id} value={candidate.id}>{candidate.title}</option>)}</select> : null}
    </div>
    {action.description ? <p className="text-sm text-muted-foreground">{action.description}</p> : null}
    {inputSchema && (inputSchema.type === 'object' || record(inputSchema.properties))
      ? <PluginConfigEditor schema={inputSchema} value={input} onChange={updateInput} locale={locale} />
      : <Textarea aria-label={t('pluginLifecycle.fields.config')} minRows={4} value={input} onValueChange={updateInput} classNames={{ input: 'font-mono text-xs' }} />}
    {!parsed.valid ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.pages.actions.invalidInput')}</p> : null}
    {proposal.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.pages.actions.proposalError')}</p> : null}
    {confirm.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.pages.actions.confirmError')}</p> : null}
    {proposalStatus && !resultStatus ? <p role="status" className="text-xs text-muted-foreground">{t('pluginLifecycle.pages.actions.pending', { status: proposalStatus })}</p> : null}
    {resultStatus ? <p role="status" className="text-sm font-medium">{t('pluginLifecycle.pages.actions.completed', { status: resultStatus })}</p> : null}
    <Button color="warning" variant="flat" isDisabled={!parsed.valid || proposal.isPending || finished} onPress={createProposal}><ShieldCheck size={15} />{t(proposal.isPending ? 'pluginLifecycle.pages.actions.proposing' : 'pluginLifecycle.pages.actions.review')}</Button>
    <Modal isOpen={modalOpen} isDismissable={!confirm.isPending} isKeyboardDismissDisabled={confirm.isPending} onOpenChange={(open) => { if (!open && !confirm.isPending) setModalOpen(false) }}>
      <ModalContent>
        <ModalHeader>{t('pluginLifecycle.pages.actions.confirmTitle', { action: action.title })}</ModalHeader>
        <ModalBody><p className="text-sm leading-6">{t('pluginLifecycle.pages.actions.confirmDescription')}</p><pre className="max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-lg bg-content1/50 p-3 font-mono text-xs">{JSON.stringify(parsed.value, null, 2)}</pre>{confirm.isError ? <p role="alert" className="text-sm text-danger">{t('pluginLifecycle.pages.actions.confirmError')}</p> : null}</ModalBody>
        <ModalFooter><Button variant="light" isDisabled={confirm.isPending} onPress={() => setModalOpen(false)}>{t('pluginLifecycle.cancel')}</Button><Button color="warning" isDisabled={confirm.isPending || !proposalKey || !confirmationKey} onPress={executeProposal}><ShieldCheck size={15} />{t(confirm.isPending ? 'pluginLifecycle.pages.actions.executing' : 'pluginLifecycle.pages.actions.confirm')}</Button></ModalFooter>
      </ModalContent>
    </Modal>
  </section>
}
