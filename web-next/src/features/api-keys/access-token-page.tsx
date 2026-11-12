import { useEffect, useMemo, useState } from 'react'
import { Button, Chip, Skeleton } from '@heroui/react'
import { KeyRound, Pencil, Plus, RefreshCw, ShieldCheck, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useManagementSession } from '@/features/auth/session-query'
import { copyText } from '@/lib/clipboard'
import { cn } from '@/lib/utils'
import {
  useAccessTokenCatalog, useAccessTokenPolicy, useAccessTokens, useCreateAccessToken,
  useRevokeAccessToken, useRevokeAllUserAccessTokens, useUpdateAccessToken,
  useUpdateAccessTokenPolicy, useRevokeOwnAccessTokens, type AccessToken, type AccessTokenScope,
  type AccessTokenIdentity, type AccessTokenWrite, requestAccessTokenProof,
} from './access-token-api'
import { clearPendingDraft, loadPendingDraft, timestamp, type AccessTokenDraft } from './access-token-model'
import { AccessTokenEditor } from './access-token-editor'
import { AdminTokenControls } from './access-token-admin'
import { IssuedAccessTokenDialog, RevokeAccessTokenDialog, RevokeAllAccessTokensDialog } from './access-token-dialogs'

export function AccessTokenPage() {
  const { i18n, t } = useTranslation()
  const session = useManagementSession(true)
  const identity: AccessTokenIdentity | undefined = session.data?.user ? {
    userId: session.data.user.id,
    role: session.data.user.role,
  } : undefined
  const isAdmin = identity?.role === 'admin'
  const tokensQuery = useAccessTokens(identity)
  const catalogQuery = useAccessTokenCatalog(identity)
  const policyQuery = useAccessTokenPolicy(identity)
  const createMutation = useCreateAccessToken()
  const updateMutation = useUpdateAccessToken()
  const revokeMutation = useRevokeAccessToken()
  const revokeOwnMutation = useRevokeOwnAccessTokens()
  const updatePolicy = useUpdateAccessTokenPolicy(identity)
  const revokeAll = useRevokeAllUserAccessTokens()
  const [editor, setEditor] = useState<AccessToken | 'create'>()
  const [revoking, setRevoking] = useState<AccessToken>()
  const [revokingAll, setRevokingAll] = useState(false)
  const [issuedSecret, setIssuedSecret] = useState<{ token: AccessToken; secret: string }>()
  const [pendingDraft] = useState(loadPendingDraft)
  const [activeDraft, setActiveDraft] = useState<AccessTokenDraft>()
  const [revokeUserId, setRevokeUserId] = useState('')
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const tokens = useMemo(() => tokensQuery.data?.pages.flatMap((page) => page.tokens) ?? [], [tokensQuery.data])
  const catalog = catalogQuery.data
  const capacity = tokensQuery.data?.pages[0]?.capacity
  const atCapacity = capacity !== undefined && tokens.length >= capacity
  const groupedScopes = useMemo(() => {
    const groups = new Map<string, AccessTokenScope[]>()
    for (const scope of catalog?.scopes ?? []) {
      const group = scope.scope.split(':', 1)[0]
      groups.set(group, [...(groups.get(group) ?? []), scope])
    }
    return [...groups.entries()]
  }, [catalog?.scopes])

  const [draftRestored, setDraftRestored] = useState(false)
  const openEditor = (value: AccessToken | 'create') => {
    createMutation.reset()
    updateMutation.reset()
    setActiveDraft(undefined)
    setEditor(value)
  }
  const closeEditor = () => {
    setActiveDraft(undefined)
    setEditor(undefined)
  }
  useEffect(() => {
    if (!pendingDraft || draftRestored || !tokensQuery.isSuccess || session.data?.user.id !== pendingDraft.owner_id) return
    if (pendingDraft.token_id === undefined) {
      setEditor('create')
      setActiveDraft(pendingDraft)
      setDraftRestored(true)
      clearPendingDraft()
      return
    }
    const token = tokens.find((item) => item.id === pendingDraft.token_id)
    if (token) {
      setEditor(token)
      setActiveDraft(pendingDraft)
      setDraftRestored(true)
      clearPendingDraft()
    } else if (tokensQuery.hasNextPage) {
      void tokensQuery.fetchNextPage()
    } else if (!tokensQuery.isFetching) {
      setDraftRestored(true)
      clearPendingDraft()
    }
  }, [pendingDraft, draftRestored, session.data?.user.id, tokens, tokensQuery.fetchNextPage, tokensQuery.hasNextPage, tokensQuery.isFetching, tokensQuery.isSuccess])

  const updatePolicyValue = (patch: Partial<Omit<NonNullable<typeof policyQuery.data>, 'version'>>) => {
    if (!policyQuery.data) return
    void updatePolicy.mutateAsync({ body: { ...policyQuery.data, ...patch }, version: policyQuery.data.version }).catch(() => undefined)
  }

  const submitEditor = async (body: AccessTokenWrite, token: AccessToken | undefined, elevated: boolean) => {
    const payload: AccessTokenWrite = token
      ? { name: body.name, scopes: body.scopes, expires_at: body.expires_at, allow_ips: body.allow_ips, enabled: body.enabled, version: token.version }
      : { name: body.name, scopes: body.scopes, expires_at: body.expires_at, allow_ips: body.allow_ips }
    const proofResult = elevated ? await requestAccessTokenProof({
      action: token ? 'update' : 'create', token_id: token?.id,
      payload,
    }) : undefined
    const authorizedBody = { ...payload, proof: proofResult?.proof }
    if (token) {
      await updateMutation.mutateAsync({ id: token.id, body: authorizedBody })
    } else {
      const result = await createMutation.mutateAsync(authorizedBody)
      setIssuedSecret({ token: result.token, secret: result.secret })
      setCopyState('idle')
      createMutation.reset()
    }
    setEditor(undefined)
  }

  const revokeToken = async () => {
    if (!revoking) return
    try {
      await revokeMutation.mutateAsync(revoking.id)
      setRevoking(undefined)
    } catch {
      // Keep the confirmation open so the user can retry.
    }
  }

  const revokeUserTokens = async () => {
    if (!/^[1-9]\d*$/.test(revokeUserId)) return
    try {
      await revokeAll.mutateAsync(Number(revokeUserId))
      setRevokeUserId('')
    } catch {
      // Keep the entered user ID and expose the stable error below.
    }
  }

  return (
    <div className="grid gap-5">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('accessTokens.title')}</h2>
          <p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">{t('accessTokens.subtitle')}</p>
        </div>
        <div className="flex gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={tokensQuery.isFetching} onPress={() => void tokensQuery.refetch()}>
            <RefreshCw className={cn('size-3.5', tokensQuery.isFetching && 'animate-spin')} aria-hidden="true" />{t('accessTokens.actions.refresh')}
          </Button>
          <Button type="button" size="sm" color="primary" isDisabled={atCapacity || tokensQuery.isPending || !catalog || policyQuery.data?.enabled === false || policyQuery.data?.issuance_enabled === false} onPress={() => openEditor('create')}>
            <Plus className="size-3.5" aria-hidden="true" />{t('accessTokens.actions.create')}
          </Button>
        </div>
      </header>

      <div className="flex flex-wrap items-center justify-between gap-3 border-y border-[var(--hairline)] py-2 text-xs text-muted-foreground">
        <span>{t('accessTokens.count', { count: tokens.length, capacity: capacity ?? '—' })}</span>
        <div className="flex items-center gap-2">{capacity !== undefined ? <Chip size="sm" variant="flat" className={capacity <= tokens.length ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'}>{t('accessTokens.capacity', { count: tokens.length, capacity })}</Chip> : null}{tokens.length > 0 ? <Button type="button" size="sm" variant="light" className="text-destructive" onPress={() => { revokeOwnMutation.reset(); setRevokingAll(true) }}>{t('accessTokens.actions.revokeAll')}</Button> : null}</div>
      </div>

      <div role="note" className="flex items-start gap-2 border-l-2 border-info px-3 py-2 text-xs leading-5 text-muted-foreground">
        <ShieldCheck className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
        <p>{t('accessTokens.modelKeyIndependence')}</p>
      </div>

      {catalogQuery.isError ? <InlineError message={t('accessTokens.errors.catalog')} onRetry={() => void catalogQuery.refetch()} /> : null}
      {tokensQuery.isPending ? <div className="grid gap-2" aria-label={t('accessTokens.loading')}>{[0, 1, 2].map((row) => <Skeleton key={row} className="h-16 rounded-md" />)}</div> : null}
      {tokensQuery.isError ? <InlineError message={t('accessTokens.errors.list')} onRetry={() => void tokensQuery.refetch()} /> : null}
      {tokensQuery.isSuccess && tokens.length === 0 ? <div className="border-y border-[var(--hairline)] py-10 text-center"><KeyRound className="mx-auto size-6 text-muted-foreground" aria-hidden="true" /><p className="mt-2 text-sm font-medium">{t('accessTokens.empty.title')}</p><p className="mt-1 text-xs text-muted-foreground">{t('accessTokens.empty.body')}</p></div> : null}

      {tokens.length > 0 ? (
        <div className="divide-y divide-[var(--hairline)] border-y border-[var(--hairline)]">
          {tokens.map((token) => <AccessTokenRow key={token.id} token={token} locale={i18n.language} onEdit={() => openEditor(token)} onRevoke={() => { revokeMutation.reset(); setRevoking(token) }} />)}
        </div>
      ) : null}
      {tokensQuery.hasNextPage ? <Button type="button" size="sm" variant="bordered" className="justify-self-center" isDisabled={tokensQuery.isFetchingNextPage} onPress={() => void tokensQuery.fetchNextPage()}>{t('accessTokens.actions.loadMore')}</Button> : null}

      {isAdmin ? (
        <AdminTokenControls
          key={policyQuery.data?.version ?? 'pending-policy'}
          policy={policyQuery.data}
          loading={policyQuery.isPending}
          onChange={updatePolicyValue}
          userId={revokeUserId}
          onUserIdChange={setRevokeUserId}
          onRevokeUser={() => void revokeUserTokens()}
          pending={updatePolicy.isPending || revokeAll.isPending}
          error={policyQuery.isError || updatePolicy.isError || revokeAll.isError}
          revokedCount={revokeAll.data?.revoked}
        />
      ) : null}

      <AccessTokenEditor
        key={editor === 'create' ? 'create' : editor?.id ?? 'closed'}
        token={editor === 'create' ? undefined : editor}
        ownerId={session.data?.user.id}
        isAdmin={isAdmin}
        catalog={catalog}
        groupedScopes={groupedScopes}
        isOpen={editor !== undefined}
        pending={createMutation.isPending || updateMutation.isPending}
        error={createMutation.error ?? updateMutation.error}
        initialDraft={activeDraft}
        onClose={closeEditor}
        onSubmit={(body, elevated) => submitEditor(body, editor === 'create' ? undefined : editor, elevated)}
      />

      <IssuedAccessTokenDialog issued={issuedSecret} copyState={copyState} onCopy={async () => setCopyState(await copyText(issuedSecret?.secret ?? '') ? 'copied' : 'failed')} onClose={() => setIssuedSecret(undefined)} />
      <RevokeAccessTokenDialog token={revoking} pending={revokeMutation.isPending} failed={revokeMutation.isError} onClose={() => setRevoking(undefined)} onConfirm={() => void revokeToken()} />
      <RevokeAllAccessTokensDialog open={revokingAll} pending={revokeOwnMutation.isPending} failed={revokeOwnMutation.isError} onClose={() => setRevokingAll(false)} onConfirm={() => void revokeOwnMutation.mutateAsync().then(() => setRevokingAll(false)).catch(() => undefined)} />
    </div>
  )
}

function AccessTokenRow({ token, locale, onEdit, onRevoke }: { token: AccessToken; locale: string; onEdit: () => void; onRevoke: () => void }) {
  const { t } = useTranslation()
  const expired = token.expires_at !== null && token.expires_at <= Date.now() / 1000
  const status = expired ? 'expired' : token.enabled ? 'enabled' : 'disabled'
  const groups = [...new Set(token.scopes.map((scope) => scope.split(':', 1)[0]))]
  return (
    <article className="grid gap-3 py-3 sm:grid-cols-[minmax(0,1.2fr)_minmax(0,2fr)_minmax(12rem,auto)] sm:items-center">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2"><KeyRound className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" /><h3 className="truncate text-sm font-medium">{token.name}</h3><Chip size="sm" variant="flat" className={status === 'enabled' ? 'bg-success/10 text-success' : status === 'expired' ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'}>{t(`accessTokens.status.${status}`)}</Chip></div>
        <p className="mt-1 pl-6 font-mono text-[11px] text-muted-foreground">{token.token_hint}</p>
      </div>
      <div className="flex min-w-0 flex-wrap gap-1">{groups.slice(0, 5).map((group) => <Chip key={group} size="sm" variant="flat" className="max-w-full bg-surface-2 text-muted-foreground"><span className="truncate">{t(`accessTokens.scopeGroups.${group}`, { defaultValue: group })}</span></Chip>)}{groups.length > 5 ? <Chip size="sm" variant="flat" className="bg-surface-2 text-muted-foreground">+{groups.length - 5}</Chip> : null}</div>
      <div className="flex flex-wrap items-center justify-between gap-2 sm:justify-end">
        <div className="min-w-0 text-[11px] leading-4 text-muted-foreground"><p>{t('accessTokens.expiresAt', { value: timestamp(token.expires_at, locale, t('accessTokens.never')) })}</p><p>{t('accessTokens.lastUsed', { value: timestamp(token.last_used_at, locale, t('accessTokens.neverUsed')) })}{token.last_used_ip ? ` · ${token.last_used_ip}` : ''}</p></div>
        <div className="flex shrink-0 gap-1"><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('accessTokens.actions.edit')} onPress={onEdit}><Pencil className="size-3.5" aria-hidden="true" /></Button><Button type="button" isIconOnly size="sm" variant="light" aria-label={t('accessTokens.actions.revoke')} className="text-destructive" onPress={onRevoke}><Trash2 className="size-3.5" aria-hidden="true" /></Button></div>
      </div>
    </article>
  )
}

function InlineError({ message, onRetry }: { message: string; onRetry: () => void }) {
  const { t } = useTranslation()
  return <div role="alert" className="flex flex-wrap items-center justify-between gap-2 border-y border-destructive/25 py-3 text-xs text-destructive"><span>{message}</span><Button type="button" size="sm" variant="bordered" onPress={onRetry}>{t('accessTokens.actions.retry')}</Button></div>
}

