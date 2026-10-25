import { useState } from 'react'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useSoftwareStatus, useSoftwareUpdate } from './software-update-api'

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
  const managedBusy = status?.launch_mode === 'launcher' && ['preparing', 'restarting', 'starting', 'backoff', 'waiting_previous_process'].includes(status.update_phase)
  const completed = status?.launch_mode === 'launcher' && status.update_phase === 'idle' && requestedTag === `v${status.version}`
  const busy = check.isPending || install.isPending || managedBusy
  return <section aria-labelledby="software-update-title" className="rounded-xl border border-[var(--hairline)] p-4">
    <div className="flex flex-wrap items-center justify-between gap-3">
      <h3 id="software-update-title" className="text-sm font-semibold">{t('softwareUpdate.title')}</h3>
      {status?.check_enabled && <Button type="button" size="sm" variant="flat" isDisabled={busy} onClick={() => { setConfirmed(false); install.reset(); check.mutate(undefined) }}>{check.isPending ? t('softwareUpdate.checking') : t('softwareUpdate.check')}</Button>}
    </div>
    {query.isPending && <p role="status" className="mt-3 text-sm text-muted-foreground">{t('softwareUpdate.loading')}</p>}
    {query.isError && !managedBusy && <div role="alert" className="mt-3 text-sm text-destructive">{t('softwareUpdate.loadError')}<Button type="button" size="sm" variant="flat" onClick={() => void query.refetch()}>{t('softwareUpdate.retry')}</Button></div>}
    {status && <div className="mt-3 grid gap-3 text-sm">
      <dl className="grid gap-x-6 gap-y-2 break-all sm:grid-cols-2">
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.edition')}</dt><dd>{status.edition} · {status.version}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.core')}</dt><dd>{status.core_version}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.deployment')}</dt><dd>{status.deployment} · {status.target}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.launchMode')}</dt><dd>{t(status.launch_mode === 'launcher' ? 'softwareUpdate.fields.launcher' : 'softwareUpdate.fields.standalone')}{status.launcher_version ? ` · ${status.launcher_version}` : ''}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.node')}</dt><dd>{status.node_id}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.commit')}</dt><dd>{status.commit || t('softwareUpdate.fields.notProvided')}</dd></div>
        <div><dt className="text-muted-foreground">{t('softwareUpdate.fields.buildTime')}</dt><dd>{status.build_time || t('softwareUpdate.fields.notProvided')}</dd></div>
      </dl>
      <p className="text-xs text-muted-foreground">{t('softwareUpdate.nodeScope')}</p>
      {status.install_blocked_reason && <p role="status" className="text-xs text-muted-foreground">{t(blocked[status.install_blocked_reason] || 'softwareUpdate.blocked.default')}</p>}
      {managedBusy && <p role="status" className="font-medium">{t('softwareUpdate.managedBusy')}</p>}
      {completed && <p role="status" className="font-medium">{t('softwareUpdate.completed', { version: status.version })}<Button type="button" size="sm" variant="flat" onClick={() => window.location.reload()}>{t('softwareUpdate.refreshPage')}</Button></p>}
      {status.launcher_error && <p role="alert" className="text-destructive">{t('softwareUpdate.launcherError', { error: status.launcher_error })}</p>}
      {status.restart_required && <p role="status" className="font-medium">{t('softwareUpdate.restartRequired')}</p>}
      {typeof status.checked_at === 'number' && <p className="text-xs text-muted-foreground">{t('softwareUpdate.checkedAt', { time: new Date(status.checked_at * 1000).toLocaleString() })}{!status.update_available && ` · ${t('softwareUpdate.noUpdate')}`}</p>}
      {status.latest && <div className="grid gap-3 border-t border-[var(--hairline)] pt-3">
        <a href={status.latest.url} target="_blank" rel="noopener noreferrer" className="font-medium underline">{t(status.update_available ? 'softwareUpdate.latestAvailable' : 'softwareUpdate.latestRelease')}：{status.latest.version}</a>
        {status.latest.package_url ? <a href={status.latest.package_url} className="break-all text-xs underline">{t('softwareUpdate.downloadPackage', { name: status.latest.package_name })}</a> : <p className="text-xs text-muted-foreground">{t('softwareUpdate.noPackage')}</p>}
        <details><summary className="cursor-pointer text-xs">{t('softwareUpdate.releaseNotes')}</summary><pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words font-sans text-xs">{status.latest.notes || t('softwareUpdate.noReleaseNotes')}</pre></details>
        {status.update_available && status.deployment === 'binary' && status.latest.package_name && !status.restart_required && !managedBusy && <>
          {status.launch_mode !== 'launcher' && <code className="overflow-auto rounded bg-muted px-2 py-2 text-xs">anyflows update install {status.latest.tag} --yes</code>}
          {status.install_enabled && <>
            <label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={confirmed} disabled={busy} onChange={(event) => setConfirmed(event.target.checked)} />{t(status.launch_mode === 'launcher' ? 'softwareUpdate.confirmLauncher' : 'softwareUpdate.confirmStandalone')}</label>
            <Button type="button" size="sm" isDisabled={!confirmed || busy} onClick={() => { setRequestedTag(status.latest!.tag); install.mutate({ tag: status.latest!.tag, nodeId: status.node_id }) }}>{install.isPending ? t('softwareUpdate.installing') : t('softwareUpdate.install')}</Button>
          </>}
        </>}
      </div>}
    </div>}
    {(check.isError || (install.isError && !completed && !managedBusy)) && <p role="alert" className="mt-3 text-sm text-destructive">{install.isError ? t('softwareUpdate.installError') : t('softwareUpdate.checkError')}</p>}
  </section>
}
