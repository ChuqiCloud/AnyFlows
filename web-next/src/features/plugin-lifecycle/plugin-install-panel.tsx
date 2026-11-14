import { Button, Card, CardBody, CardHeader, Textarea } from '@heroui/react'
import { Check, Upload } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import { useInstallPlugin } from './plugin-lifecycle-api'

export function PluginInstallPanel({ disabled = false, onInstallingChange }: { disabled?: boolean; onInstallingChange?: (installing: boolean) => void }) {
  const { t } = useTranslation()
  const install = useInstallPlugin()
  const [packageFile, setPackageFile] = useState<File>()
  const [config, setConfig] = useState('{}')
  const [fileInputVersion, setFileInputVersion] = useState(0)
  const [validationError, setValidationError] = useState<'packageType' | 'configJson'>()
  const busy = disabled || install.isPending
  useEffect(() => onInstallingChange?.(install.isPending), [install.isPending, onInstallingChange])

  const submit = () => {
    if (!packageFile || busy) return
    if (!/\.(zip|afplugin)$/i.test(packageFile.name)) {
      setValidationError('packageType')
      return
    }
    let parsed: unknown
    try {
      parsed = JSON.parse(config)
    } catch {
      setValidationError('configJson')
      return
    }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      setValidationError('configJson')
      return
    }
    setValidationError(undefined)
    install.mutate({ packageFile, config: parsed as Record<string, unknown> }, {
      onSuccess: () => {
        setPackageFile(undefined)
        setConfig('{}')
        setFileInputVersion((version) => version + 1)
      },
    })
  }

  const errorKey = install.error instanceof ApiError
    ? install.error.status === 422 ? 'needsReview'
      : install.error.status === 409 ? 'conflict'
      : install.error.status === 413 ? 'tooLarge'
        : install.error.status === 400 ? 'invalidPackage'
          : install.error.status === 403 ? 'forbidden'
            : 'failed'
    : 'failed'

  return <Card shadow="none" className="rounded-xl border border-[var(--hairline)] bg-card">
    <CardHeader className="p-5 pb-0"><div><h2 className="flex items-center gap-2 text-base font-semibold"><Upload size={17} />{t('pluginLifecycle.install.title')}</h2><p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.install.description')}</p></div></CardHeader>
    <CardBody className="gap-4 p-5">
      <label className="grid gap-2 text-sm font-medium">
        {t('pluginLifecycle.install.package')}
        <input key={fileInputVersion} type="file" accept=".zip,.afplugin,application/zip,application/x-zip-compressed" disabled={busy} className="block w-full rounded-lg border border-[var(--hairline)] bg-background p-2 text-sm file:mr-3 file:cursor-pointer file:rounded-md file:border-0 file:bg-surface-2 file:px-3 file:py-2 file:text-sm file:font-medium disabled:opacity-50" onChange={(event) => {
          setPackageFile(event.target.files?.[0])
          setValidationError(undefined)
          install.reset()
        }} />
        <span className="text-xs font-normal text-muted-foreground">{t('pluginLifecycle.install.packageHint')}</span>
      </label>
      <details className="text-sm"><summary className="cursor-pointer font-medium">{t('pluginLifecycle.install.configure')}</summary><div className="pt-3"><Textarea label={t('pluginLifecycle.fields.config')} description={t('pluginLifecycle.configHint')} minRows={4} value={config} onValueChange={(value) => { setConfig(value); setValidationError(undefined); install.reset() }} isDisabled={busy} classNames={{ input: 'font-mono text-xs' }} /></div></details>
      {validationError ? <p role="alert" className="text-sm text-danger">{t(`pluginLifecycle.install.errors.${validationError}`)}</p> : null}
      {install.isError ? <p role="alert" className="rounded-lg border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">{t(`pluginLifecycle.install.errors.${errorKey}`)}</p> : null}
      {install.isSuccess ? <p role="status" className="flex items-center gap-2 rounded-lg border border-success/30 bg-success/5 px-3 py-2 text-sm text-success"><Check size={16} />{t('pluginLifecycle.install.success', { plugin: install.data.pluginId, version: install.data.artifact.pluginVersion })}</p> : null}
      <div className="flex justify-end"><Button color="primary" isLoading={install.isPending} isDisabled={busy || !packageFile} onPress={submit}>{t(install.isPending ? 'pluginLifecycle.install.installing' : 'pluginLifecycle.install.submit')}</Button></div>
    </CardBody>
  </Card>
}
