import { AlertTriangle, RefreshCw, type LucideIcon } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { cn } from '@/lib/utils'

export function TopupLoading() {
  return <Skeleton className="h-72 rounded-xl" />
}

export function TopupUnavailable({ onRetry, retryable }: { onRetry: () => void; retryable: boolean }) {
  const { t } = useTranslation()
  return (
    <Card>
      <CardContent className="flex flex-col gap-4 p-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex min-w-0 gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-warning/10 text-warning">
            <AlertTriangle className="size-4" aria-hidden="true" />
          </span>
          <div>
            <h3 className="text-sm font-semibold">{t('wallet.topup.unavailable.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t(retryable ? 'wallet.topup.unavailable.retryable' : 'wallet.topup.unavailable.notConfigured')}
            </p>
          </div>
        </div>
        {retryable ? <Button type="button" size="sm" variant="secondary" onClick={onRetry}><RefreshCw aria-hidden="true" />{t('wallet.actions.retry')}</Button> : null}
      </CardContent>
    </Card>
  )
}

export function TopupState({
  action,
  body,
  icon: Icon,
  spinning = false,
  title,
  tone,
}: {
  action?: ReactNode
  body: string
  icon: LucideIcon
  spinning?: boolean
  title: string
  tone: 'brand' | 'destructive' | 'success' | 'warning'
}) {
  return (
    <div className="grid min-h-52 place-items-center text-center">
      <div className="max-w-sm">
        <span className={cn(
          'mx-auto grid size-10 place-items-center rounded-lg bg-surface-2',
          tone === 'brand' && 'text-brand',
          tone === 'destructive' && 'text-destructive',
          tone === 'success' && 'text-success',
          tone === 'warning' && 'text-warning',
        )}>
          <Icon className={cn('size-4', spinning && 'animate-spin')} aria-hidden="true" />
        </span>
        <h4 className="mt-3 text-sm font-semibold">{title}</h4>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
        {action ? <div className="mt-4">{action}</div> : null}
      </div>
    </div>
  )
}
