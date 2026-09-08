import { LoaderCircle, RefreshCw, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

type VideoTaskAccessPanelProps = {
  apiKeyReady: boolean
  apiKeyProvisioning: 'idle' | 'pending' | 'failed'
  onProvisionApiKey: () => void
}

/** 展示平台托管会话状态，视频任务不接受手动 Key。 */
export function VideoTaskAccessPanel(props: VideoTaskAccessPanelProps) {
  const { t } = useTranslation()
  const keyStatus = props.apiKeyReady
    ? 'ready'
    : props.apiKeyProvisioning === 'pending'
      ? 'provisioning'
      : props.apiKeyProvisioning === 'failed'
        ? 'failed'
        : 'missing'

  return (
    <section className="grid gap-3 p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-xs font-semibold">{t('videoTasks.access.title')}</h3>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {t('videoTasks.access.summary')}
          </p>
        </div>
        <Badge className={keyStatus === 'ready' ? 'bg-success/12 text-success' : 'bg-warning/12 text-warning'}>
          {keyStatus === 'provisioning' ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(`videoTasks.apiKey.${keyStatus}`)}
        </Badge>
      </div>

      <div className="flex gap-2.5 rounded-md border border-[var(--hairline)] bg-surface-2/60 p-2.5">
        <ShieldCheck className="mt-0.5 size-4 shrink-0 text-primary" aria-hidden="true" />
        <p className="text-[0.6875rem] leading-5 text-muted-foreground">
          {t('videoTasks.access.description')}
        </p>
      </div>
      {keyStatus === 'failed' || keyStatus === 'missing' ? (
        <Button type="button" size="sm" variant="secondary" onClick={props.onProvisionApiKey}>
          <RefreshCw aria-hidden="true" />{t('videoTasks.apiKey.retry')}
        </Button>
      ) : null}
    </section>
  )
}
