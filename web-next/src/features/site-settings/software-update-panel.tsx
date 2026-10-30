import { useState } from 'react'
import { ArrowUpRight, Check, ChevronDown, CircleAlert, RefreshCw, Sparkles } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button, Checkbox } from '@heroui/react'
import { useSoftwareStatus, useSoftwareUpdate } from './software-update-api'
import { SoftwareReleaseNotes } from './software-release-notes'

const blocked: Record<string, string> = {
  container: 'softwareUpdate.blocked.container',
  not_enabled: 'softwareUpdate.blocked.not_enabled',
  unsupported_target: 'softwareUpdate.blocked.unsupported_target',
  restart_required: 'softwareUpdate.blocked.restart_required',
  executable_unavailable: 'softwareUpdate.blocked.executable_unavailable',
  host_managed: 'softwareUpdate.blocked.host_managed',
  launcher_unavailable: 'softwareUpdate.blocked.launcher_unavailable',
  managed_restart: 'softwareUpdate.blocked.managed_restart',
}

export function SoftwareUpdatePanel() {
  const { t } = useTranslation()
  const query = useSoftwareStatus()
  const check = useSoftwareUpdate('check')
  const install = useSoftwareUpdate('install')
  const [confirmed, setConfirmed] = useState(false)
  const [requestedTag, setRequestedTag] = useState<string>()
  const status = query.data
  const release = status?.latest
  const managedBusy = status?.launch_mode === 'launcher' && ['preparing', 'restarting', 'starting', 'backoff', 'waiting_previous_process'].includes(status.update_phase)
  const completed = status?.launch_mode === 'launcher' && status.update_phase === 'idle' && requestedTag === `v${status.version}`
  const busy = check.isPending || install.isPending || managedBusy
  const hasChecked = check.isSuccess || typeof status?.checked_at === 'number'
  const canInstall = status?.deployment === 'binary' && Boolean(release?.package_name) && status.install_enabled

  return (
    <section aria-labelledby="software-update-title" className="min-w-0 overflow-hidden rounded-3xl border border-[var(--hairline)] bg-surface-1 shadow-sm">
      <div className="p-5 sm:p-8">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <h3 id="software-update-title" className="text-base font-semibold tracking-tight">{t('softwareUpdate.title')}</h3>
          {status?.check_enabled && (
            <Button type="button" size="sm" variant="flat" isDisabled={busy} onClick={() => {
              setConfirmed(false)
              setRequestedTag(undefined)
              install.reset()
              check.mutate(undefined)
            }}>
              <RefreshCw aria-hidden="true" className={`size-3.5 ${check.isPending ? 'motion-safe:animate-spin' : ''}`} />
              {check.isPending ? t('softwareUpdate.checking') : t('softwareUpdate.check')}
            </Button>
          )}
        </div>

        {query.isPending && <p role="status" className="mt-6 text-sm text-muted-foreground">{t('softwareUpdate.loading')}</p>}
        {query.isError && !managedBusy && (
          <div role="alert" className="mt-4 flex flex-wrap items-center gap-3 rounded-xl bg-destructive/5 p-3 text-sm text-destructive">
            <CircleAlert className="size-4 shrink-0" aria-hidden="true" />
            <p>{t('softwareUpdate.loadError')}</p>
            <Button type="button" size="sm" variant="flat" onClick={() => void query.refetch()}>{t('softwareUpdate.retry')}</Button>
          </div>
        )}

        {status && (
          <>
            <div className="mt-6 flex flex-wrap items-end justify-between gap-4">
              <div className="min-w-0">
                <p className="mb-2 text-xs font-medium text-muted-foreground">{t('softwareUpdate.currentVersion')}</p>
                <div className="flex flex-wrap items-center gap-3">
                  <p className="break-all text-3xl font-semibold tracking-tight sm:text-4xl">{status.version}</p>
                  <span className="rounded-full border border-[var(--hairline)] bg-surface-2/60 px-2.5 py-1 text-xs font-medium text-muted-foreground">
                    {t(`softwareUpdate.editions.${status.edition}`, { defaultValue: status.edition })}
                  </span>
                </div>
              </div>
              {typeof status.checked_at === 'number' && (
                <p className="text-xs leading-5 text-muted-foreground">{t('softwareUpdate.checkedAt', { time: new Date(status.checked_at * 1000).toLocaleString() })}</p>
              )}
            </div>

            <div role="status" aria-live="polite" aria-atomic="true">
              {check.isPending ? (
                <p className="mt-5 flex items-center gap-2 rounded-xl bg-surface-2/60 px-4 py-3 text-sm text-muted-foreground">
                  <RefreshCw aria-hidden="true" className="size-4 shrink-0 motion-safe:animate-spin" />
                  {t('softwareUpdate.checking')}
                </p>
              ) : !check.isError && !query.isError && hasChecked && (
                <p className={`mt-5 flex items-start gap-2.5 rounded-xl px-4 py-3 text-sm leading-6 ${status.update_available ? 'bg-primary/5 text-primary' : 'bg-success/5 text-success'}`}>
                  {status.update_available ? <Sparkles aria-hidden="true" className="mt-1 size-4 shrink-0" /> : <Check aria-hidden="true" className="mt-1 size-4 shrink-0" />}
                  {status.update_available ? t('softwareUpdate.checkAvailable', { version: release?.version || '' }) : t('softwareUpdate.checkComplete')}
                </p>
              )}
            </div>
            {check.isError && <p role="alert" className="mt-5 rounded-xl bg-destructive/5 px-4 py-3 text-sm leading-6 text-destructive">{t('softwareUpdate.checkError')}</p>}

            <details className="group mt-5 border-t border-[var(--hairline)] pt-4">
              <summary className="flex w-fit cursor-pointer list-none items-center gap-2 rounded-sm text-xs font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">
                {t('softwareUpdate.versionDetails')}
                <ChevronDown aria-hidden="true" className="size-3.5 transition-transform group-open:rotate-180" />
              </summary>
              <dl className="mt-4 grid gap-x-8 gap-y-4 rounded-xl bg-surface-2/40 p-4 text-xs sm:grid-cols-2">
                {[
                  ['core', status.core_version],
                  ['deployment', `${status.deployment} · ${status.target}`],
                  ['launchMode', `${t(status.launch_mode === 'launcher' ? 'softwareUpdate.fields.launcher' : 'softwareUpdate.fields.standalone')}${status.launcher_version ? ` · ${status.launcher_version}` : ''}`],
                  ['node', status.node_id],
                  ['commit', status.commit || t('softwareUpdate.fields.notProvided')],
                  ['buildTime', status.build_time || t('softwareUpdate.fields.notProvided')],
                ].map(([field, value]) => (
                  <div key={field} className="min-w-0 space-y-1.5">
                    <dt className="text-muted-foreground">{t(`softwareUpdate.fields.${field}`)}</dt>
                    <dd className="break-all leading-5">{value}</dd>
                  </div>
                ))}
              </dl>
            </details>

            {(managedBusy || completed || status.restart_required || status.launcher_error) && (
              <div className="mt-5 space-y-3 rounded-xl border border-[var(--hairline)] bg-surface-2/40 p-4 text-sm leading-6">
                {managedBusy && <p role="status">{t('softwareUpdate.managedBusy')}</p>}
                {completed && <div role="status" className="flex flex-wrap items-center gap-3 text-success"><p>{t('softwareUpdate.completed', { version: status.version })}</p><Button type="button" size="sm" variant="flat" onClick={() => window.location.reload()}>{t('softwareUpdate.refreshPage')}</Button></div>}
                {status.launcher_error && <p role="alert" className="break-words text-destructive">{t('softwareUpdate.launcherError', { error: status.launcher_error })}</p>}
                {status.restart_required && <p role="status">{t('softwareUpdate.restartRequired')}</p>}
              </div>
            )}
          </>
        )}
      </div>

      {status && release && (
        <div className="border-t border-[var(--hairline)]">
          <div className="flex flex-wrap items-center justify-between gap-4 bg-surface-2/30 px-5 py-5 sm:px-8">
            <div className="min-w-0">
              <p className="mb-1.5 text-xs font-medium text-muted-foreground">{t(status.update_available ? 'softwareUpdate.latestAvailable' : 'softwareUpdate.latestRelease')}</p>
              <h4 className="break-all text-xl font-semibold tracking-tight">{release.version}</h4>
            </div>
            <a href={release.url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 rounded-sm text-xs font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring">
              {t('softwareUpdate.viewRelease')}<ArrowUpRight aria-hidden="true" className="size-3.5" />
            </a>
          </div>
          <div className="min-w-0 px-5 py-6 sm:px-8">
            <p className="mb-5 text-xs font-semibold text-muted-foreground">{t('softwareUpdate.releaseNotes')}</p>
            {release.notes.trim() ? <SoftwareReleaseNotes content={release.notes} /> : <p className="text-sm text-muted-foreground">{t('softwareUpdate.noReleaseNotes')}</p>}
          </div>

          {status.update_available && !status.restart_required && !managedBusy && (
            <div className="space-y-4 border-t border-[var(--hairline)] bg-surface-2/20 px-5 py-5 text-sm leading-6 sm:px-8">
              {(status.install_blocked_reason || !canInstall) && (
                <p role="status" className="text-muted-foreground">{t(status.install_blocked_reason ? blocked[status.install_blocked_reason] || 'softwareUpdate.blocked.default' : 'softwareUpdate.inlineUnavailable')}</p>
              )}
              {canInstall && (
                <>
                  <Checkbox className="text-xs text-muted-foreground" isDisabled={busy} isSelected={confirmed} size="sm" onValueChange={setConfirmed}>
                    {t(status.launch_mode === 'launcher' ? 'softwareUpdate.confirmLauncher' : 'softwareUpdate.confirmStandalone')}
                  </Checkbox>
                  <div className="flex flex-wrap items-center justify-between gap-3">
                    <p className="max-w-lg text-xs text-muted-foreground">{t('softwareUpdate.nodeScope')}</p>
                    <Button type="button" size="sm" isDisabled={!confirmed || busy} onClick={() => {
                      check.reset()
                      setRequestedTag(release.tag)
                      install.mutate({ tag: release.tag, nodeId: status.node_id })
                    }}>{install.isPending ? t('softwareUpdate.installing') : t('softwareUpdate.install')}</Button>
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}
      {install.isError && !completed && !managedBusy && <p role="alert" className="mx-5 mb-5 rounded-xl bg-destructive/5 px-4 py-3 text-sm leading-6 text-destructive sm:mx-8">{t('softwareUpdate.installError')}</p>}
    </section>
  )
}
