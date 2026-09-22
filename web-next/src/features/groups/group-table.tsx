import { Button, Chip } from '@heroui/react'
import { Layers3, Pencil, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminGroup } from '@/lib/api/generated/types.gen'
import { SiteTooltip } from '@/shared/components/site-tooltip'
import { ratioFromMicros } from './group-form-model'
import { GroupQuotaWindows } from './group-quota-windows'

type GroupTableProps = {
  groups: AdminGroup[]
  onDelete: (group: AdminGroup) => void
  onEdit: (group: AdminGroup) => void
}

function GroupBadges({ group }: { group: AdminGroup }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      {group.is_exclusive ? <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('groups.badges.exclusive')}</Chip> : null}
      {group.flags.claude_code_only === true ? <Chip size="sm" variant="flat">{t('groups.badges.claudeCodeOnly')}</Chip> : null}
      {group.peak_ratio_micros !== null ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('groups.badges.peak')}</Chip> : null}
    </div>
  )
}

function PricingValue({ group }: { group: AdminGroup }) {
  const { t } = useTranslation()
  return (
    <div className="tabular-nums">
      <div className="font-medium">{t('groups.values.ratio', { value: ratioFromMicros(group.ratio_micros) })}</div>
      {group.peak_ratio_micros !== null ? (
        <div className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
          {t('groups.values.peakWindow', {
            ratio: ratioFromMicros(group.peak_ratio_micros),
            start: shortTime(group.peak_start),
            end: shortTime(group.peak_end),
          })}
        </div>
      ) : <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('groups.values.noPeak')}</div>}
    </div>
  )
}

function RoutingValue({ group, groupNames }: { group: AdminGroup; groupNames: ReadonlyMap<number, string> }) {
  const { t } = useTranslation()
  const fallback = group.fallback_group_id === null
    ? t('groups.values.noFallback')
    : groupNames.get(group.fallback_group_id) ?? `#${group.fallback_group_id}`
  return (
    <div>
      <div className="truncate font-medium">{fallback}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('groups.values.fallback')}</div>
    </div>
  )
}

function RowActions({ group, onDelete, onEdit }: {
  group: AdminGroup
  onDelete: (group: AdminGroup) => void
  onEdit: (group: AdminGroup) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <SiteTooltip content={t('groups.actions.edit')}>
        <Button isIconOnly aria-label={t('groups.actions.edit')} className="size-10 md:size-8" size="sm" type="button" variant="light" onClick={() => onEdit(group)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
      <SiteTooltip content={t('groups.actions.delete')}>
        <Button isIconOnly aria-label={t('groups.actions.delete')} className="size-10 text-muted-foreground hover:text-destructive md:size-8" size="sm" type="button" variant="light" onClick={() => onDelete(group)}><Trash2 className="size-3.5" aria-hidden="true" /></Button>
      </SiteTooltip>
    </div>
  )
}

export function GroupTable({ groups, onDelete, onEdit }: GroupTableProps) {
  const { t } = useTranslation()
  const groupNames = new Map(groups.map((group) => [group.id, group.display_name]))
  if (groups.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-t border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Layers3 className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('groups.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.empty.body')}</p>
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
              <th className="w-[22%] px-3 py-2 font-medium">{t('groups.columns.group')}</th>
              <th className="w-[18%] px-3 py-2 font-medium">{t('groups.columns.pricing')}</th>
              <th className="w-[30%] px-3 py-2 font-medium">{t('groups.columns.limits')}</th>
              <th className="w-[20%] px-3 py-2 font-medium">{t('groups.columns.routing')}</th>
              <th className="w-[10%] px-3 py-2"><span className="sr-only">{t('groups.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {groups.map((group) => (
              <tr key={group.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5">
                  <div className="truncate font-medium">{group.display_name}</div>
                  <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{group.name} · #{group.id}</div>
                  <div className="mt-1.5"><GroupBadges group={group} /></div>
                </td>
                <td className="px-3 py-2.5"><PricingValue group={group} /></td>
                <td className="px-3 py-2.5"><GroupQuotaWindows group={group} /></td>
                <td className="px-3 py-2.5"><RoutingValue group={group} groupNames={groupNames} /></td>
                <td className="px-2 py-2.5"><RowActions group={group} onDelete={onDelete} onEdit={onEdit} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {groups.map((group) => (
          <article key={group.id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h2 className="truncate text-sm font-semibold">{group.display_name}</h2>
                <p className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">{group.name} · #{group.id}</p>
                <div className="mt-2"><GroupBadges group={group} /></div>
              </div>
              <RowActions group={group} onDelete={onDelete} onEdit={onEdit} />
            </div>
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
              <PricingValue group={group} />
              <RoutingValue group={group} groupNames={groupNames} />
            </div>
            <div className="mt-3 border-t border-[var(--hairline)] pt-3"><GroupQuotaWindows group={group} /></div>
          </article>
        ))}
      </div>
    </>
  )
}

function shortTime(value: string | null) {
  return value?.endsWith(':00') ? value.slice(0, -3) : value ?? ''
}
