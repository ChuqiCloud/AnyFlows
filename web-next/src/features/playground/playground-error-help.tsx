import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import type { PlaygroundErrorKind } from './playground-types'

type HelpLink = { path: string; key: string }

const helpLinks: Record<PlaygroundErrorKind, HelpLink[]> = {
  invalid_key: [{ path: '/console/api-keys', key: 'playground.diagnostics.manageKeys' }],
  access_restricted: [{ path: '/console/api-keys', key: 'playground.diagnostics.checkAccess' }],
  account_verification_required: [{ path: '/console/account-verification', key: 'playground.diagnostics.verification' }],
  insufficient_quota: [
    { path: '/console/api-keys', key: 'playground.diagnostics.checkKeyQuota' },
    { path: '/console/wallet', key: 'playground.diagnostics.checkBalance' },
  ],
  rate_limited: [{ path: '/console/api-keys', key: 'playground.diagnostics.checkKeyLimits' }],
  model_unavailable: [
    { path: '/console/models', key: 'playground.diagnostics.browseModels' },
    { path: '/console/api-keys', key: 'playground.diagnostics.checkModelAccess' },
  ],
  invalid_request: [{ path: '/api', key: 'playground.diagnostics.apiReference' }],
  upstream_unavailable: [{ path: '/console/usage-logs', key: 'playground.diagnostics.requestLogs' }],
  unknown: [{ path: '/console/usage-logs', key: 'playground.diagnostics.requestLogs' }],
}

export function PlaygroundErrorHelp({ kind }: { kind: PlaygroundErrorKind }) {
  const { t } = useTranslation()
  return <div className="grid justify-items-start gap-1.5">
    <p className="text-[11px] leading-4 text-muted-foreground">{t(`playground.diagnostics.${kind}`)}</p>
    <div className="flex flex-wrap gap-x-3 gap-y-1">{helpLinks[kind].map(({ path, key }) => <Link key={path} className="text-[11px] font-medium text-primary hover:underline" to={path}>{t(key)}</Link>)}</div>
  </div>
}
