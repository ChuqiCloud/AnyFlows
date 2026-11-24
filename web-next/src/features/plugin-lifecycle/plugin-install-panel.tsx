import { Button, Card, CardBody, CardHeader, Input } from '@heroui/react'
import { Check, GitBranch, Store, Upload } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import { useInstallPlugin } from './plugin-lifecycle-api'
import { PluginMarketPanel } from './plugin-market-panel'

const installSources = [
  { id: 'package', icon: Upload },
  { id: 'repository', icon: GitBranch },
  { id: 'market', icon: Store },
] as const

type InstallSource = typeof installSources[number]['id']

function isGitHubRepository(value: string) {
  return /^https:\/\/github\.com\/[a-z0-9._-]{1,100}\/[a-z0-9._-]{1,100}(?:\.git)?\/?$/i.test(value)
}

function installationErrorKey(status: number | undefined, source: InstallSource) {
  switch (status) {
    case 400:
      return source === 'repository' ? 'invalidRepositoryPackage' : 'invalidPackage'
    case 401:
      return 'sessionExpired'
    case 403:
      return 'forbidden'
    case 404:
      return source === 'repository' ? 'repositoryNotFound' : 'failed'
    case 409:
      return 'conflict'
    case 413:
      return 'tooLarge'
    case 422:
      return 'needsReview'
    case 503:
      return source === 'repository' ? 'repositoryUnavailable' : 'failed'
    default:
      return 'failed'
  }
}

export function PluginInstallPanel({ disabled = false, onInstallingChange }: {
  disabled?: boolean
  onInstallingChange?: (installing: boolean) => void
}) {
  const { t } = useTranslation()
  const install = useInstallPlugin()
  const [source, setSource] = useState<InstallSource>('package')
  const [packageFile, setPackageFile] = useState<File>()
  const [repository, setRepository] = useState('')
  const [assetName, setAssetName] = useState('')
  const [fileInputVersion, setFileInputVersion] = useState(0)
  const [marketInstalling, setMarketInstalling] = useState(false)
  const [validationError, setValidationError] = useState<'packageType' | 'repositoryUrl'>()
  const installing = install.isPending || marketInstalling
  const busy = disabled || installing

  useEffect(() => onInstallingChange?.(installing), [installing, onInstallingChange])

  const clearFeedback = () => {
    setValidationError(undefined)
    install.reset()
  }

  const submit = () => {
    if (busy || source === 'market') return
    if (source === 'package' && (!packageFile || !/\.(zip|afplugin)$/i.test(packageFile.name))) {
      setValidationError('packageType')
      return
    }
    if (source === 'repository' && !isGitHubRepository(repository.trim())) {
      setValidationError('repositoryUrl')
      return
    }

    setValidationError(undefined)
    const selected = source === 'package' && packageFile
      ? { packageFile }
      : { repository: repository.trim(), assetName: assetName.trim() || undefined }
    install.mutate(selected, {
      onSuccess: () => {
        if (source === 'package') {
          setPackageFile(undefined)
          setFileInputVersion((version) => version + 1)
        }
      },
    })
  }

  const errorStatus = install.error instanceof ApiError ? install.error.status : undefined
  const errorKey = installationErrorKey(errorStatus, source)
  const validationText = validationError ? t('pluginLifecycle.install.errors.' + validationError) : undefined

  return <Card id="plugin-install" shadow="none" className="scroll-mt-6 rounded-xl border border-[var(--hairline)] bg-card">
    <CardHeader className="px-5 pb-0 pt-5">
      <div>
        <h2 className="text-base font-semibold">{t('pluginLifecycle.install.title')}</h2>
        <p className="mt-1 text-sm text-muted-foreground">{t('pluginLifecycle.install.description')}</p>
      </div>
    </CardHeader>
    <CardBody className="gap-4 p-5">
      <fieldset className="flex flex-wrap gap-2" disabled={busy}>
        <legend className="sr-only">{t('pluginLifecycle.install.sourceLabel')}</legend>
        {installSources.map(({ id, icon: Icon }) => (
          <label key={id} className="cursor-pointer has-[:disabled]:cursor-default">
            <input
              className="peer sr-only"
              type="radio"
              name="plugin-install-source"
              value={id}
              checked={source === id}
              onChange={() => {
                setSource(id)
                clearFeedback()
              }}
            />
            <span className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-divider px-3 text-sm font-medium hover:bg-content2 peer-checked:border-primary/50 peer-checked:bg-primary/10 peer-checked:text-primary peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-primary peer-disabled:opacity-60">
              <Icon size={16} aria-hidden="true" />{t('pluginLifecycle.install.sources.' + id)}
            </span>
          </label>
        ))}
      </fieldset>

      <div hidden={source !== 'market'}>
        <PluginMarketPanel disabled={busy} onInstallingChange={setMarketInstalling} />
      </div>
      <div hidden={source === 'market'}>
        <form className="grid gap-4" noValidate onSubmit={(event) => { event.preventDefault(); submit() }}>
          <div hidden={source !== 'package'}>
            <label className="grid gap-2 text-sm font-medium">
              {t('pluginLifecycle.install.package')}
              <input key={fileInputVersion} type="file" accept=".zip,.afplugin,application/zip,application/x-zip-compressed" disabled={busy || source !== 'package'} className="block w-full rounded-lg border border-divider bg-background p-2 text-sm file:mr-3 file:cursor-pointer file:rounded-md file:border-0 file:bg-surface-2 file:px-3 file:py-2 file:text-sm file:font-medium disabled:opacity-50" onChange={(event) => {
                setPackageFile(event.target.files?.[0])
                clearFeedback()
              }} />
              <span className="text-xs font-normal text-muted-foreground">{t('pluginLifecycle.install.packageHint')}</span>
            </label>
            {validationError === 'packageType' ? <p role="alert" className="mt-2 text-sm text-danger">{validationText}</p> : null}
          </div>
          {source === 'repository' ? (
            <Input label={t('pluginLifecycle.install.repository')} type="url" placeholder="https://github.com/owner/plugin"
              value={repository} maxLength={256} isDisabled={busy} isInvalid={validationError === 'repositoryUrl'}
              description={t('pluginLifecycle.install.repositoryHint')}
              errorMessage={validationError === 'repositoryUrl' ? validationText : undefined}
              onValueChange={(value) => { setRepository(value); clearFeedback() }} />
          ) : null}

          {source === 'repository' ? <Input label={t('pluginLifecycle.install.assetName')}
            description={t('pluginLifecycle.install.assetNameHint')} maxLength={255} value={assetName}
            isDisabled={busy} onValueChange={(value) => { setAssetName(value); clearFeedback() }} /> : null}
          {install.isError ? <p role="alert" className="rounded-lg border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">{t('pluginLifecycle.install.errors.' + errorKey)}</p> : null}
          {install.isSuccess ? <p role="status" className="flex items-center gap-2 rounded-lg border border-success/30 bg-success/5 px-3 py-2 text-sm text-success">
            <Check size={16} className="shrink-0" aria-hidden="true" />
            <span className="break-all">
              {t(install.data.needsConfiguration ? 'pluginLifecycle.install.successNeedsConfiguration' : 'pluginLifecycle.install.success', { plugin: install.data.pluginId, version: install.data.artifact.pluginVersion })}
            </span>
          </p> : null}
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-xs text-muted-foreground">{t('pluginLifecycle.trustWarning')}</p>
            <Button className="ms-auto" type="submit" color="primary" isLoading={install.isPending}
              isDisabled={busy || (source === 'repository' ? !repository.trim() : !packageFile)}>
              {t(install.isPending ? 'pluginLifecycle.install.installing' : 'pluginLifecycle.install.submit')}
            </Button>
          </div>
        </form>
      </div>
    </CardBody>
  </Card>
}
