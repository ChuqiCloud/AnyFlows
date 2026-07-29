import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Select } from '@/components/ui/select'
import { useAdminGroupCatalog } from '@/features/groups/group-api'

type ChannelGroupSelectorProps = {
  id: string
  value: number[]
  invalid?: boolean
  onChange: (value: number[]) => void
}

/** 从真实分组目录选择路由范围，已选项使用可移除徽标呈现。 */
export function ChannelGroupSelector({ id, value, invalid, onChange }: ChannelGroupSelectorProps) {
  const { t } = useTranslation()
  const groupsQuery = useAdminGroupCatalog()
  const groups = groupsQuery.data ?? []
  const groupsById = new Map(groups.map((group) => [group.id, group]))

  return (
    <div className="grid gap-2">
      <Select
        id={id}
        value=""
        disabled={groupsQuery.isPending || groupsQuery.isError || value.length >= 64}
        aria-invalid={invalid}
        onChange={(event) => {
          const idValue = Number(event.target.value)
          if (Number.isSafeInteger(idValue) && !value.includes(idValue)) onChange([...value, idValue])
        }}
      >
        <option value="">
          {groupsQuery.isPending
            ? t('channels.form.groupsLoading')
            : groupsQuery.isError
              ? t('channels.form.groupsLoadFailed')
              : t('channels.form.groupPlaceholder')}
        </option>
        {groups.map((group) => (
          <option key={group.id} value={group.id} disabled={value.includes(group.id)}>
            {group.display_name} ({group.name})
          </option>
        ))}
      </Select>
      {value.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {value.map((groupId) => {
            const group = groupsById.get(groupId)
            const label = group ? group.display_name : `#${groupId}`
            return (
              <Badge key={groupId} className="h-7 gap-1 border-transparent bg-success/10 pl-2 text-success" title={group?.name}>
                <span className="max-w-48 truncate">{label}</span>
                <button
                  type="button"
                  className="grid size-5 place-items-center rounded-sm hover:bg-success/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  aria-label={t('channels.actions.removeGroup', { group: label })}
                  title={t('channels.actions.removeGroup', { group: label })}
                  onClick={() => onChange(value.filter((idValue) => idValue !== groupId))}
                >
                  <X className="size-3" aria-hidden="true" />
                </button>
              </Badge>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
