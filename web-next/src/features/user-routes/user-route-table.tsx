import { Button, Chip } from '@heroui/react'
import { Pencil, Route, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { SiteTooltip } from '@/shared/components/site-tooltip'
import type { SelectableTokenGroup } from '@/features/api-keys/api-key-group-model'
import { formatUserRoutePriceRatio, type UserRoute } from './user-route-model'

type UserRouteTableProps = {
  routes: UserRoute[]
  groups: SelectableTokenGroup[]
  onEdit: (route: UserRoute) => void
  onDelete: (route: UserRoute) => void
}

function RouteSummary({ route }: { route: UserRoute }) {
  const { t } = useTranslation()
  return (
    <div className="min-w-0">
      <div className="flex items-center gap-2">
        <span className="truncate font-medium" title={route.name}>{route.name}</span>
        <Chip size="sm" variant="flat" className={route.enabled ? 'shrink-0 bg-success/10 text-success' : 'shrink-0 bg-surface-2 text-muted-foreground'}>
          {t(route.enabled ? 'userRoutes.enabled' : 'userRoutes.disabled')}
        </Chip>
      </div>
      <div className="mt-1 font-mono text-[0.6875rem] text-muted-foreground">
        #{route.id} · {t('userRoutes.attemptsValue', { count: route.max_group_attempts })}
      </div>
    </div>
  )
}

function RouteStrategy({ route }: { route: UserRoute }) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="font-medium">{t(`userRoutes.strategies.${route.strategy}`)}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {route.max_price_ratio_micros === null
          ? t('userRoutes.unlimitedPrice')
          : t('userRoutes.priceValue', { value: formatUserRoutePriceRatio(route.max_price_ratio_micros) })}
      </div>
    </div>
  )
}

function RouteMembers({ route, names }: { route: UserRoute; names: Map<number, string> }) {
  return (
    <div className="flex flex-wrap gap-1">
      {route.members.map((member) => (
        <Chip key={member.group_id} size="sm" variant="flat" classNames={{ base: 'min-w-0 max-w-full', content: 'min-w-0 max-w-44 truncate' }}>
          {names.get(member.group_id) ?? `#${member.group_id}`}
        </Chip>
      ))}
    </div>
  )
}

function RouteActions({ route, onEdit, onDelete }: Pick<UserRouteTableProps, 'onEdit' | 'onDelete'> & { route: UserRoute }) {
  const { t } = useTranslation()
  return (
    <div className="flex shrink-0 items-center justify-end gap-1">
      <SiteTooltip content={t('userRoutes.actions.edit')}>
        <Button type="button" isIconOnly aria-label={t('userRoutes.actions.edit')} className="size-9" size="sm" variant="light" onPress={() => onEdit(route)}>
          <Pencil className="size-3.5" aria-hidden="true" />
        </Button>
      </SiteTooltip>
      <SiteTooltip content={t('userRoutes.actions.delete')}>
        <Button type="button" isIconOnly aria-label={t('userRoutes.actions.delete')} className="size-9 text-muted-foreground hover:text-danger" size="sm" variant="light" onPress={() => onDelete(route)}>
          <Trash2 className="size-3.5" aria-hidden="true" />
        </Button>
      </SiteTooltip>
    </div>
  )
}

export function UserRouteTable({ routes, groups, onEdit, onDelete }: UserRouteTableProps) {
  const { t } = useTranslation()
  const names = new Map(groups.map((group) => [group.id, group.display_name] as const))
  if (routes.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-sm">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Route className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('userRoutes.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('userRoutes.empty.body')}</p>
        </div>
      </div>
    )
  }
  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[25%] px-3 py-2 font-medium">{t('userRoutes.columns.route')}</th>
              <th className="w-[20%] px-3 py-2 font-medium">{t('userRoutes.columns.strategy')}</th>
              <th className="w-[40%] px-3 py-2 font-medium">{t('userRoutes.columns.members')}</th>
              <th className="w-[15%] px-3 py-2"><span className="sr-only">{t('userRoutes.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {routes.map((route) => (
              <tr key={route.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><RouteSummary route={route} /></td>
                <td className="px-3 py-2.5"><RouteStrategy route={route} /></td>
                <td className="px-3 py-2.5"><RouteMembers route={route} names={names} /></td>
                <td className="px-2 py-2.5"><RouteActions route={route} onEdit={onEdit} onDelete={onDelete} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="grid gap-2 md:hidden">
        {routes.map((route) => (
          <article key={route.id} className="rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <RouteSummary route={route} />
              <RouteActions route={route} onEdit={onEdit} onDelete={onDelete} />
            </div>
            <div className="mt-3 border-t border-[var(--hairline)] pt-3 text-xs"><RouteStrategy route={route} /></div>
            <div className="mt-3"><RouteMembers route={route} names={names} /></div>
          </article>
        ))}
      </div>
    </>
  )
}
