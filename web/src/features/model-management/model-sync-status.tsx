import { useTranslation } from 'react-i18next'

/** 使用服务端闭合错误码呈现可操作信息，不暴露底层网络诊断。 */
export function ModelSyncError({ code }: { code: string }) {
  const { t } = useTranslation()
  return (
    <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
      {t(`modelManagement.sync.errors.${code}`, { defaultValue: t('modelManagement.sync.errors.unknown') })}
    </p>
  )
}

export function currentUnix() {
  return Math.floor(Date.now() / 1000)
}

export function formatModelSyncExpiry(timestamp: number) {
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(timestamp * 1000))
}
