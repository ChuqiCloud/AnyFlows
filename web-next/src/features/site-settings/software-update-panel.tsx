import { useState } from 'react'
import { Button } from '@heroui/react'
import { useSoftwareStatus, useSoftwareUpdate } from './software-update-api'

const blocked: Record<string, string> = {
  container: '容器部署仅支持检查版本，请使用新的镜像标签重新部署。',
  not_enabled: '在线更新已通过配置关闭，需重新开启后安装。',
  unsupported_target: '当前平台没有对应的安装包，请按部署文档手动升级。',
  restart_required: '程序已安装，请重启当前节点以运行新版本。',
  executable_unavailable: '无法定位当前程序，请手动升级。',
  host_managed: '当前部署暂不开放在线更新。',
  launcher_unavailable: '启动器控制通道不可用，请检查托管进程。',
  managed_restart: '已交由启动器更新，完成后自动重新启动当前节点。',
}

export function SoftwareUpdatePanel() {
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
      <h3 id="software-update-title" className="text-sm font-semibold">版本与更新</h3>
      {status?.check_enabled && <Button type="button" size="sm" variant="flat" isDisabled={busy} onClick={() => { setConfirmed(false); install.reset(); check.mutate(undefined) }}>{check.isPending ? '正在检查…' : '检查更新'}</Button>}
    </div>
    {query.isPending && <p role="status" className="mt-3 text-sm text-muted-foreground">正在读取版本信息…</p>}
    {query.isError && !managedBusy && <div role="alert" className="mt-3 text-sm text-destructive">无法读取版本信息。<Button type="button" size="sm" variant="flat" onClick={() => void query.refetch()}>重试</Button></div>}
    {status && <div className="mt-3 grid gap-3 text-sm">
      <dl className="grid gap-x-6 gap-y-2 break-all sm:grid-cols-2">
        <div><dt className="text-muted-foreground">产品版本</dt><dd>{status.edition} · {status.version}</dd></div>
        <div><dt className="text-muted-foreground">内核版本</dt><dd>{status.core_version}</dd></div>
        <div><dt className="text-muted-foreground">部署平台</dt><dd>{status.deployment} · {status.target}</dd></div>
        <div><dt className="text-muted-foreground">启动方式</dt><dd>{status.launch_mode === 'launcher' ? '启动器托管（自动重启）' : '裸启动（手动重启）'}{status.launcher_version ? ` · ${status.launcher_version}` : ''}</dd></div>
        <div><dt className="text-muted-foreground">当前节点</dt><dd>{status.node_id}</dd></div>
        <div><dt className="text-muted-foreground">源码提交</dt><dd>{status.commit || '未提供'}</dd></div>
        <div><dt className="text-muted-foreground">构建时间</dt><dd>{status.build_time || '未提供'}</dd></div>
      </dl>
      <p className="text-xs text-muted-foreground">检测、安装与重启均针对当前节点；多节点部署请逐台处理。安装前备份数据库、配置与计费 WAL，二进制回退不等于数据库降级。</p>
      {status.install_blocked_reason && <p role="status" className="text-xs text-muted-foreground">{blocked[status.install_blocked_reason] || '当前部署不支持在线安装。'}</p>}
      {managedBusy && <p role="status" className="font-medium">启动器托管更新中，完成后将自动重启当前节点。等待请求排空期间可能短暂不可用，请稍候。</p>}
      {completed && <p role="status" className="font-medium">主程序已由启动器更新至 {status.version}。<Button type="button" size="sm" variant="flat" onClick={() => window.location.reload()}>刷新页面</Button></p>}
      {status.launcher_error && <p role="alert" className="text-destructive">启动器报告未完成：{status.launcher_error}。请检查进程日志，程序不会跨数据库迁移自动回退。</p>}
      {status.restart_required && <p role="status" className="font-medium">安装完成，重启后生效。旧程序保留在程序目录的 anyflows-backup-* 文件中。</p>}
      {typeof status.checked_at === 'number' && <p className="text-xs text-muted-foreground">上次检查：{new Date(status.checked_at * 1000).toLocaleString()}（检查结果缓存一分钟）{!status.update_available && ' · 未发现更新版本'}</p>}
      {status.latest && <div className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3">
        <a href={status.latest.url} target="_blank" rel="noopener noreferrer" className="font-medium underline">{status.update_available ? '发现新版本' : '最新发行版本'}：{status.latest.version}</a>
        {status.latest.package_url ? <a href={status.latest.package_url} className="break-all text-xs underline">下载匹配安装包：{status.latest.package_name}</a> : <p className="text-xs text-muted-foreground">该版本没有当前平台的安装包，不会使用其他平台的文件。</p>}
        <details><summary className="cursor-pointer text-xs">查看更新内容</summary><pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap break-words font-sans text-xs">{status.latest.notes || '未提供更新说明。'}</pre></details>
        {status.update_available && status.deployment === 'binary' && status.latest.package_name && !status.restart_required && !managedBusy && <>
          {status.launch_mode !== 'launcher' && <code className="overflow-auto rounded bg-muted px-2 py-2 text-xs">anyflows update install {status.latest.tag} --yes</code>}
          {status.install_enabled && <>
            <label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={confirmed} disabled={busy} onChange={(event) => setConfirmed(event.target.checked)} />{status.launch_mode === 'launcher' ? '我已备份数据库、配置与 WAL，了解只更新当前节点，安装期间短暂不可用并由启动器自动重启。' : '我已备份数据库、配置与 WAL，了解只替换当前节点程序，安装后需要手动重启。'}</label>
            <Button type="button" size="sm" isDisabled={!confirmed || busy} onClick={() => { setRequestedTag(status.latest!.tag); install.mutate({ tag: status.latest!.tag, nodeId: status.node_id }) }}>{install.isPending ? '正在下载并校验，请勿关闭页面…' : '下载并安装'}</Button>
          </>}
        </>}
      </div>}
    </div>}
    {(check.isError || (install.isError && !completed && !managedBusy)) && <p role="alert" className="mt-3 text-sm text-destructive">{install.isError ? '安装未完成，请核对版本、程序目录权限及网络后重试。' : '检测失败，请检查网络或稍后重试；GitHub API 可能暂时限流。'}</p>}
  </section>
}
