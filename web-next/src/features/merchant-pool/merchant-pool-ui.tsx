import type { ReactNode } from 'react'
import { Button, Chip, Skeleton, type ButtonProps } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { cn } from '@/lib/utils'

export function PoolButton({ className, color = 'default', variant = 'solid', ...props }: ButtonProps) {
  const neutral = color === 'default' || color === 'secondary'
  const plain = variant === 'light' || variant === 'bordered' || variant === 'ghost' || variant === 'faded'
  const accent = color === 'primary' ? 'text-primary' : color === 'danger' ? 'text-destructive' : color === 'success' ? 'text-success' : color === 'warning' ? 'text-warning' : undefined
  const flat = color === 'primary' ? 'bg-primary/12 text-primary' : color === 'danger' ? 'bg-destructive/12 text-destructive' : color === 'success' ? 'bg-success/12 text-success' : color === 'warning' ? 'bg-warning/12 text-warning' : 'bg-surface-2 text-foreground'
  return <Button {...props} color={color} variant={variant} className={cn(neutral && 'text-foreground', neutral && (variant === 'solid' || variant === 'shadow') && 'bg-surface-2', neutral && variant === 'bordered' && 'border-[var(--hairline)]', plain && (neutral ? 'data-[hover=true]:bg-surface-2' : accent), variant === 'flat' && flat, className)} />
}

export function PoolHeader({ title, description, refreshing, onRefresh, children }: { title: string; description: string; refreshing: boolean; onRefresh: () => void; children?: ReactNode }) {
  const { t } = useTranslation()
  return <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between"><div><h2 className="text-lg font-semibold">{title}</h2><p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">{description}</p></div><div className="flex flex-wrap items-center gap-2"><PoolButton type="button" size="sm" variant="bordered" isDisabled={refreshing} onPress={onRefresh}><RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} aria-hidden="true" />{t('merchantPool.refresh')}</PoolButton>{children}</div></header>
}

export function PoolNavigation() {
  const { t } = useTranslation()
  return <nav aria-label={t('merchantPool.title')} className="flex flex-wrap gap-x-4 gap-y-2 border-b border-[var(--hairline)] pb-3 text-xs"><Link className="text-primary hover:underline" to="/console/merchant-pool">{t('merchantPool.title')}</Link><Link className="text-primary hover:underline" to="/console/merchant-pool/routes">{t('merchantPool.routes')}</Link><Link className="text-primary hover:underline" to="/console/merchant">{t('merchantPool.workspace')}</Link></nav>
}

export function PoolField({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return <label className="grid min-w-0 gap-1.5 text-xs"><span className="font-medium">{label}</span>{children}{hint ? <span className="leading-5 text-muted-foreground">{hint}</span> : null}</label>
}

export function PoolSelect({ label, value, onChange, options, disabled, allowEmpty = true }: { label: string; value: string; onChange: (value: string) => void; options: Array<{ value: string; label: string }>; disabled?: boolean; allowEmpty?: boolean }) {
  const { t } = useTranslation()
  return <select className="min-h-8 w-full min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-2.5 text-xs disabled:opacity-50" aria-label={label} value={value} onChange={(event) => onChange(event.target.value)} disabled={disabled}>{allowEmpty ? <option value="">{t('merchantPool.all')}</option> : null}{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select>
}

export function PoolError({ onRetry, message }: { onRetry?: () => void; message?: string }) {
  const { t } = useTranslation()
  return <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-danger/25 bg-danger/8 p-3 text-xs text-danger"><span>{message ?? t('merchantPool.loadError')}</span>{onRetry ? <PoolButton type="button" size="sm" variant="flat" onPress={onRetry}>{t('merchantPool.retry')}</PoolButton> : null}</div>
}

export function PoolLoading() { return <div className="grid gap-3" role="status"><Skeleton className="h-20 rounded-lg" /><Skeleton className="h-20 rounded-lg" /><Skeleton className="h-20 rounded-lg" /></div> }

export function PoolEmpty({ children }: { children?: ReactNode }) { const { t } = useTranslation(); return <p className="rounded-lg border border-dashed border-[var(--hairline)] p-6 text-center text-xs leading-6 text-muted-foreground">{children ?? t('merchantPool.empty')}</p> }

export function PoolStatus({ status }: { status: string }) {
  const { t } = useTranslation()
  const success = ['active', 'normal', 'published', 'approved', 'available', 'paid'].includes(status)
  const danger = ['rejected', 'suspended', 'disabled', 'platform_paused', 'failed'].includes(status)
  return <Chip size="sm" variant="flat" className={`h-5 px-1 text-[11px] ${success ? 'bg-success/10 text-success' : danger ? 'bg-danger/10 text-danger' : 'bg-warning/10 text-warning'}`}>{t(`merchantPool.status.${status}`, { defaultValue: status })}</Chip>
}

export function PoolPager({ page, hasMore, loading, onChange }: { page: number; hasMore: boolean; loading: boolean; onChange: (page: number) => void }) {
  const { t } = useTranslation()
  return <div className="flex items-center justify-end gap-3 border-t border-[var(--hairline)] pt-3 text-xs"><PoolButton size="sm" variant="flat" isDisabled={page === 0 || loading} onPress={() => onChange(page - 1)}>{t('merchantPool.previous')}</PoolButton><span>{t('merchantPool.page', { page: page + 1 })}</span><PoolButton size="sm" variant="flat" isDisabled={!hasMore || loading} onPress={() => onChange(page + 1)}>{t('merchantPool.next')}</PoolButton></div>
}
