import { useId, type ReactNode } from 'react'
import { Button, Chip, Skeleton, type ButtonProps } from '@heroui/react'
import { Briefcase, GitBranch, RefreshCw, Store } from 'lucide-react'
import { NavLink } from 'react-router-dom'
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
  const tabs = [{ to: '/console/merchant-pool', label: t('merchantPool.title'), icon: Store }, { to: '/console/merchant-pool/routes', label: t('merchantPool.routes'), icon: GitBranch }, { to: '/console/merchant', label: t('merchantPool.workspace'), icon: Briefcase }]
  return <nav aria-label={t('merchantPool.title')} className="flex min-w-0 gap-1 overflow-x-auto border-b border-[var(--hairline)] pb-2 text-xs">{tabs.map(({ to, label, icon: Icon }) => <NavLink key={to} end to={to} className={({ isActive }) => cn('inline-flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-2 font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary', isActive ? 'bg-primary/10 text-primary' : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground')}><Icon className="size-3.5" aria-hidden="true" />{label}</NavLink>)}</nav>
}

export function PoolField({ label, children, hint }: { label: string; children: ReactNode; hint?: string }) {
  return <label className="grid min-w-0 gap-1.5 text-xs"><span className="font-medium">{label}</span>{children}{hint ? <span className="leading-5 text-muted-foreground">{hint}</span> : null}</label>
}

export function PoolSelect({ label, value, onChange, options, disabled, allowEmpty = true, emptyLabel, isInvalid = false, errorMessage }: { label: string; value: string; onChange: (value: string) => void; options: Array<{ value: string; label: string }>; disabled?: boolean; allowEmpty?: boolean; emptyLabel?: string; isInvalid?: boolean; errorMessage?: string }) {
  const { t } = useTranslation()
  const errorId = useId()
  return <><select className={cn('min-h-8 w-full min-w-0 rounded-lg border bg-background px-2.5 text-xs disabled:opacity-50', isInvalid ? 'border-danger' : 'border-[var(--hairline)]')} aria-label={label} aria-invalid={isInvalid || undefined} aria-describedby={isInvalid && errorMessage ? errorId : undefined} value={value} onChange={(event) => onChange(event.target.value)} disabled={disabled}>{allowEmpty ? <option value="">{emptyLabel ?? t('merchantPool.all')}</option> : null}{options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}</select>{isInvalid && errorMessage ? <span id={errorId} className="text-[11px] leading-5 text-danger">{errorMessage}</span> : null}</>
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
