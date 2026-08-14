import { GitBranch, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminRoute } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type RouteTableProps = {
  routes: readonly AdminRoute[]
  pendingToggleId?: number
  onEdit: (route: AdminRoute) => void
  onDelete: (route: AdminRoute) => void
  onToggle: (route: AdminRoute) => void
}

const strategyTone = {
  weighted: 'bg-info/10 text-info',
  round_robin: 'bg-brand/10 text-brand',
  stable_first: 'bg-success/10 text-success',
} as const

/** 以规则、候选和运行统计为主线展示路由，桌面端表格与移动端卡片保持同一信息密度。 */
export function RouteTable({ routes, pendingToggleId, onEdit, onDelete, onToggle }: RouteTableProps) {
  const { i18n, t } = useTranslation()
  if (routes.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><GitBranch className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('routes.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('routes.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[31%] px-3 py-2 font-medium">{t('routes.columns.route')}</th>
              <th className="w-[17%] px-3 py-2 font-medium">{t('routes.columns.strategy')}</th>
              <th className="w-[15%] px-3 py-2 font-medium">{t('routes.columns.candidates')}</th>
              <th className="w-[23%] px-3 py-2 font-medium">{t('routes.columns.health')}</th>
              <th className="w-[14%] px-2 py-2"><span className="sr-only">{t('routes.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {routes.map((route) => (
              <tr key={route.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><RouteIdentity route={route} /></td>
                <td className="px-3 py-2.5"><StrategyBadges route={route} /></td>
                <td className="px-3 py-2.5"><CandidateSummary route={route} /></td>
                <td className="px-3 py-2.5"><HealthSummary route={route} language={i18n.language} /></td>
                <td className="px-2 py-2.5"><RouteActions route={route} pending={pendingToggleId === route.id} onEdit={onEdit} onDelete={onDelete} onToggle={onToggle} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {routes.map((route) => (
          <article key={route.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><RouteIdentity route={route} /><RouteActions route={route} pending={pendingToggleId === route.id} onEdit={onEdit} onDelete={onDelete} onToggle={onToggle} /></div>
            <div className="mt-3 grid gap-2 border-t border-[var(--hairline)] pt-3"><StrategyBadges route={route} /><CandidateSummary route={route} /><HealthSummary route={route} language={i18n.language} /></div>
          </article>
        ))}
      </div>
    </>
  )
}

function RouteIdentity({ route }: { route: AdminRoute }) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0">
      <div className="flex flex-wrap items-center gap-1.5"><span className="truncate font-semibold">{route.name}</span><StatusBadge enabled={route.enabled} />{route.mode === 'explicit_group' ? <Badge className="border-transparent bg-warning/10 text-warning">{t('routes.mode.explicit_group')}</Badge> : null}</div>
      <p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{route.model_pattern}</p>
      <p className="mt-1 text-[0.625rem] text-muted-foreground">#{route.id}</p>
    </div>
  )
}

function StatusBadge({ enabled }: { enabled: boolean }) {
  const { t } = useTranslation()
  return <Badge className={cn('border-transparent', enabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground')}>{t(enabled ? 'routes.status.enabled' : 'routes.status.disabled')}</Badge>
}

function StrategyBadges({ route }: { route: AdminRoute }) {
  const { t } = useTranslation()
  return <div className="flex flex-wrap gap-1"><Badge className={cn('border-transparent', strategyTone[route.strategy])}>{t(`routes.strategy.${route.strategy}`)}</Badge><Badge>{t('routes.mode.patternShort', { mode: route.mode === 'pattern' ? t('routes.mode.pattern') : t('routes.mode.explicit_group') })}</Badge></div>
}

function CandidateSummary({ route }: { route: AdminRoute }) {
  const { t } = useTranslation()
  const enabled = route.channels.filter((candidate) => candidate.enabled).length
  return <div className="flex flex-wrap gap-1"><Badge className="border-transparent bg-info/10 text-info">{t('routes.values.candidates', { count: route.channels.length })}</Badge><Badge className="border-transparent bg-success/10 text-success">{t('routes.values.enabledCandidates', { count: enabled })}</Badge></div>
}

function HealthSummary({ route, language }: { route: AdminRoute; language: string }) {
  const { t } = useTranslation()
  const success = route.channels.reduce((sum, candidate) => sum + candidate.success_count, 0)
  const failed = route.channels.reduce((sum, candidate) => sum + candidate.fail_count, 0)
  const total = success + failed
  const percent = total > 0 ? Math.round((success / total) * 100) : undefined
  const latencyTotal = route.channels.reduce((sum, candidate) => sum + candidate.total_latency_ms, 0)
  const latencySamples = route.channels.reduce((sum, candidate) => sum + candidate.success_count + candidate.fail_count, 0)
  const latency = latencySamples > 0 ? Math.round(latencyTotal / latencySamples) : undefined
  return (
    <div className="grid gap-1 text-[0.6875rem] tabular-nums">
      <span className={percent === undefined ? 'text-muted-foreground' : percent >= 95 ? 'text-success' : percent >= 80 ? 'text-warning' : 'text-destructive'}>{percent === undefined ? t('routes.health.noData') : t('routes.health.successRate', { value: percent })}</span>
      <span className="text-muted-foreground">{latency === undefined ? t('routes.health.noLatency') : t('routes.health.latency', { value: new Intl.NumberFormat(language).format(latency) })}</span>
    </div>
  )
}

function RouteActions({ route, pending, onEdit, onDelete, onToggle }: {
  route: AdminRoute
  pending: boolean
  onEdit: (route: AdminRoute) => void
  onDelete: (route: AdminRoute) => void
  onToggle: (route: AdminRoute) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <Tooltip><TooltipTrigger asChild><span><Switch checked={route.enabled} disabled={pending} aria-label={t(route.enabled ? 'routes.actions.disable' : 'routes.actions.enable')} onCheckedChange={() => onToggle(route)} /></span></TooltipTrigger><TooltipContent>{t(route.enabled ? 'routes.actions.disable' : 'routes.actions.enable')}</TooltipContent></Tooltip>
      <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" aria-label={t('routes.actions.edit')} onClick={() => onEdit(route)}><Pencil aria-hidden="true" /></Button></TooltipTrigger><TooltipContent>{t('routes.actions.edit')}</TooltipContent></Tooltip>
      <Tooltip><TooltipTrigger asChild><Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={t('routes.actions.delete')} onClick={() => onDelete(route)}><Trash2 aria-hidden="true" /></Button></TooltipTrigger><TooltipContent>{t('routes.actions.delete')}</TooltipContent></Tooltip>
    </div>
  )
}
