import { Button, Card, CardBody, CardHeader, Chip, Input, Select, SelectItem, Skeleton } from '@heroui/react'
import { useState, type FormEvent } from 'react'
import { Check, CircleAlert, Clock3, Link2, LoaderCircle, RefreshCw, Unlink } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import {
  type ExternalAccount,
  type ExternalBalance,
  type ExternalConnector,
  useExternalAccounts,
  useExternalBalanceHistory,
  useExternalConnectors,
  useLinkExternalAccount,
  useRefreshExternalAccount,
  useUnlinkExternalAccount,
} from './external-accounts-api'

function requestErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const details = error.details as Record<string, unknown>
  return typeof details.code === 'string' ? details.code : undefined
}

function errorMessageKey(error: unknown) {
  switch (requestErrorCode(error)) {
    case 'external_account_authentication_required': return 'authentication'
    case 'external_account_conflict': return 'conflict'
    case 'external_account_unavailable': return 'unavailable'
    case 'external_account_rate_limited': return 'rateLimited'
    case 'external_account_limit_reached': return 'limitReached'
    case 'external_account_unsupported': return 'unsupported'
    default: return 'generic'
  }
}

function formatDate(value: number) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value))
}

function instanceUrl(value: string) {
  try {
    const url = new URL(value)
    const path = url.pathname.replace(/\/+$/u, '')
    return `${url.origin}${path}`
  } catch {
    return value
  }
}

function BalanceValue({ balance }: { balance: ExternalBalance }) {
  const { t } = useTranslation()
  if (balance.status !== 'available' || balance.amount === null) {
    return <Chip size="sm" variant="flat" color={balance.status === 'authentication_required' ? 'warning' : 'default'}>
      {t(`profile.externalAccounts.balanceStatus.${balance.status}`)}
    </Chip>
  }
  return <span className="font-mono text-sm font-semibold tabular-nums">{balance.amount}<span className="ml-1.5 font-sans text-xs font-normal text-muted-foreground">{balance.unit}</span></span>
}

function BalanceHistory({ accountId, open }: { accountId: number; open: boolean }) {
  const { t } = useTranslation()
  const history = useExternalBalanceHistory(accountId, open)
  if (!open) return null
  if (history.isPending) return <div className="grid gap-2 border-t border-[var(--hairline)] pt-3" role="status"><Skeleton className="h-8 rounded-lg" /><Skeleton className="h-8 rounded-lg" /></div>
  if (history.isError) return <p className="border-t border-[var(--hairline)] pt-3 text-xs text-destructive" role="alert">{t('profile.externalAccounts.historyError')}</p>
  if (!history.data.items.length) return <p className="border-t border-[var(--hairline)] pt-3 text-xs text-muted-foreground">{t('profile.externalAccounts.noHistory')}</p>
  return <ol className="grid gap-2 border-t border-[var(--hairline)] pt-3" aria-label={t('profile.externalAccounts.history')}>
    {history.data.items.map((balance) => <li key={`${balance.observedAt}:${balance.amount ?? balance.status}`} className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-default-50 px-3 py-2 text-xs">
      <time dateTime={new Date(balance.observedAt).toISOString()} className="flex items-center gap-1.5 text-muted-foreground"><Clock3 className="size-3.5" aria-hidden="true" />{formatDate(balance.observedAt)}</time>
      <BalanceValue balance={balance} />
    </li>)}
  </ol>
}

function ExternalAccountRow({ account, connectors }: { account: ExternalAccount; connectors: ExternalConnector[] }) {
  const { t } = useTranslation()
  const refresh = useRefreshExternalAccount()
  const unlink = useUnlinkExternalAccount()
  const [historyOpen, setHistoryOpen] = useState(false)
  const [confirmUnlink, setConfirmUnlink] = useState(false)
  const connectorLabel = connectors.find((connector) => connector.id === account.connectorId)?.displayName ?? account.connectorId
  const instance = instanceUrl(account.baseUrl)
  const error = refresh.error ?? unlink.error
  return <article className="grid gap-3 rounded-xl border border-[var(--hairline)] p-4">
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div className="flex min-w-0 items-center gap-3">
        <div aria-hidden="true" className="flex size-10 shrink-0 items-center justify-center rounded-xl border border-[var(--hairline)] bg-default-50 text-xs font-bold text-foreground">
          {connectorLabel.slice(0, 1).toUpperCase()}
        </div>
        <div className="min-w-0"><h4 className="truncate text-sm font-semibold">{account.displayName || account.externalSubject}</h4><p className="mt-0.5 truncate text-xs text-muted-foreground">{connectorLabel} · <span title={instance}>{instance}</span> · {t('profile.externalAccounts.linkedAt', { value: formatDate(account.createdAt * 1000) })}</p></div>
      </div>
      <div className="flex items-center gap-2">
        {account.latestBalance ? <BalanceValue balance={account.latestBalance} /> : <Chip size="sm" variant="flat">{t('profile.externalAccounts.noBalance')}</Chip>}
      </div>
    </div>
    {account.latestBalance ? <p className="text-[0.6875rem] text-muted-foreground">{t('profile.externalAccounts.observedAt', { value: formatDate(account.latestBalance.observedAt) })}</p> : null}
    {confirmUnlink ? <div className="grid gap-2 rounded-lg border border-warning/20 bg-warning/5 p-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <p className="text-xs leading-5 text-muted-foreground">{t('profile.externalAccounts.unlinkConfirm')}</p>
      <div className="flex justify-end gap-2"><Button size="sm" variant="light" isDisabled={unlink.isPending} onPress={() => { setConfirmUnlink(false); unlink.reset() }}>{t('profile.actions.cancel')}</Button><Button size="sm" color="danger" isDisabled={unlink.isPending} isLoading={unlink.isPending} onPress={() => unlink.mutate(account.id)}>{t('profile.externalAccounts.unlink')}</Button></div>
    </div> : <div className="flex flex-wrap items-center justify-end gap-2">
      <Button size="sm" variant="bordered" isDisabled={refresh.isPending} isLoading={refresh.isPending} onPress={() => refresh.mutate(account.id)}><RefreshCw className="size-3.5" aria-hidden="true" />{t('profile.externalAccounts.refresh')}</Button>
      <Button size="sm" variant="light" aria-expanded={historyOpen} onPress={() => setHistoryOpen((value) => !value)}>{historyOpen ? t('profile.externalAccounts.hideHistory') : t('profile.externalAccounts.showHistory')}</Button>
      <Button size="sm" variant="light" color="danger" onPress={() => { unlink.reset(); setConfirmUnlink(true) }}><Unlink className="size-3.5" aria-hidden="true" />{t('profile.externalAccounts.unlink')}</Button>
    </div>}
    {error ? <p role="alert" className="text-xs text-destructive">{t(`profile.externalAccounts.errors.${errorMessageKey(error)}`)}</p> : null}
    <BalanceHistory accountId={account.id} open={historyOpen} />
  </article>
}

function LinkExternalAccountForm({ connectors }: { connectors: ReturnType<typeof useExternalConnectors> }) {
  const { t } = useTranslation()
  const link = useLinkExternalAccount()
  const [connectorId, setConnectorId] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [credential, setCredential] = useState('')
  const [linkError, setLinkError] = useState<unknown>()
  const [linked, setLinked] = useState(false)
  const availableConnectors = connectors.data ?? []
  const activeConnectorId = availableConnectors.some((connector) => connector.id === connectorId)
    ? connectorId
    : availableConnectors[0]?.id ?? ''
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!activeConnectorId || !baseUrl.trim() || !credential || link.isPending) return
    let secret = credential
    setCredential('')
    setLinkError(undefined)
    setLinked(false)
    link.reset()
    try {
      await link.mutateAsync({ connector_id: activeConnectorId, base_url: baseUrl.trim(), credential: secret })
      setBaseUrl('')
      setLinked(true)
    } catch (error) {
      // Preserve the endpoint but clear the credential; the user can paste it again after correcting the issue.
      setLinkError(error)
    } finally {
      secret = ''
      // Drop mutation variables, which include the access token, as soon as the request settles.
      link.reset()
    }
  }
  const errorKey = errorMessageKey(linkError)
  return <form className="grid gap-3 rounded-xl border border-dashed border-[var(--hairline)] p-4" onSubmit={(event) => void submit(event)}>
    <div className="grid gap-3 sm:grid-cols-2">
      <div className="grid gap-1.5"><label htmlFor="profile-external-connector" className="text-xs font-medium">{t('profile.externalAccounts.platform')}</label>
        <Select id="profile-external-connector" aria-label={t('profile.externalAccounts.platform')} size="sm" selectedKeys={activeConnectorId ? [activeConnectorId] : []} onSelectionChange={(keys) => { if (keys === 'all') return; const [key] = Array.from(keys); if (typeof key === 'string') setConnectorId(key) }} isDisabled={link.isPending || connectors.isPending || !availableConnectors.length}>
          {availableConnectors.map((connector) => <SelectItem key={connector.id} textValue={connector.displayName}>{connector.displayName}</SelectItem>)}
        </Select>
      </div>
      <div className="grid gap-1.5"><label htmlFor="profile-external-url" className="text-xs font-medium">{t('profile.externalAccounts.baseUrl')}</label>
        <Input id="profile-external-url" type="url" inputMode="url" autoComplete="url" size="sm" placeholder="https://platform.example" value={baseUrl} onValueChange={(value) => { setBaseUrl(value); setLinkError(undefined); setLinked(false) }} isDisabled={link.isPending} isRequired />
      </div>
    </div>
    <div className="grid gap-1.5"><label htmlFor="profile-external-credential" className="text-xs font-medium">{t('profile.externalAccounts.credential')}</label>
      <Input id="profile-external-credential" type="password" autoComplete="off" size="sm" value={credential} onValueChange={(value) => { setCredential(value); setLinkError(undefined); setLinked(false) }} isDisabled={link.isPending} isRequired maxLength={8192} />
    </div>
    <div className="flex flex-wrap items-center justify-between gap-3">
      <p className="flex max-w-2xl items-start gap-2 text-[0.6875rem] leading-5 text-muted-foreground"><CircleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('profile.externalAccounts.safetyHint')}</p>
      <Button type="submit" size="sm" color="primary" isDisabled={!activeConnectorId || !baseUrl.trim() || !credential || link.isPending || connectors.isError} isLoading={link.isPending}>
        {link.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Link2 className="size-4" aria-hidden="true" />}{t('profile.externalAccounts.link')}
      </Button>
    </div>
    {connectors.isError ? <div className="flex items-center justify-between gap-2 text-xs text-destructive" role="alert"><span>{t('profile.externalAccounts.connectorsError')}</span><Button size="sm" variant="light" onPress={() => void connectors.refetch()}>{t('profile.actions.retry')}</Button></div> : null}
    {!connectors.isPending && !connectors.isError && !availableConnectors.length ? <p className="text-xs text-muted-foreground">{t('profile.externalAccounts.noConnectors')}</p> : null}
    {linkError ? <p className="text-xs text-destructive" role="alert">{t(`profile.externalAccounts.errors.${errorKey}`)}</p> : null}
    {linked ? <p className="flex items-center gap-1.5 text-xs text-success" role="status"><Check className="size-3.5" aria-hidden="true" />{t('profile.externalAccounts.linked')}</p> : null}
  </form>
}

export function ExternalAccountsCard() {
  const { t } = useTranslation()
  const accounts = useExternalAccounts()
  const connectors = useExternalConnectors()
  return <Card className="border border-[var(--hairline)] bg-content1" shadow="none">
    <CardHeader className="flex flex-col items-start gap-1 px-4 pb-0 pt-4 sm:px-5 sm:pt-5">
      <h3 className="text-sm font-semibold">{t('profile.externalAccounts.title')}</h3>
      <p className="text-xs leading-5 text-muted-foreground">{t('profile.externalAccounts.subtitle')}</p>
    </CardHeader>
    <CardBody className="grid gap-3 p-4 sm:p-5">
      <p className="rounded-lg bg-default-50 px-3 py-2 text-[0.6875rem] leading-5 text-muted-foreground">{t('profile.externalAccounts.walletHint')}</p>
      {accounts.isPending ? <div className="grid gap-2" role="status"><Skeleton className="h-20 rounded-xl" /><Skeleton className="h-20 rounded-xl" /></div>
        : accounts.isError ? <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/20 bg-destructive/5 p-3 text-xs" role="alert"><span>{t('profile.externalAccounts.accountsError')}</span><Button size="sm" variant="bordered" onPress={() => void accounts.refetch()}>{t('profile.actions.retry')}</Button></div>
          : accounts.data.items.length ? <div className="grid gap-3">{accounts.data.items.map((account) => <ExternalAccountRow key={account.id} account={account} connectors={connectors.data ?? []} />)}</div>
            : <p className="rounded-lg bg-default-50 px-3 py-4 text-center text-xs text-muted-foreground">{t('profile.externalAccounts.empty')}</p>}
      <LinkExternalAccountForm connectors={connectors} />
    </CardBody>
  </Card>
}
