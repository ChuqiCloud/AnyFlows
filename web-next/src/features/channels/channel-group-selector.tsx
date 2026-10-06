import { Chip, Select, SelectItem } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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

  const placeholder = groupsQuery.isPending
    ? t('channels.form.groupsLoading')
    : groupsQuery.isError
      ? t('channels.form.groupsLoadFailed')
      : t('channels.form.groupPlaceholder')

  // 仅列出尚未选中的分组；HeroUI Select 的动态选项必须走 items + 渲染函数。
  const selectable = groups.filter((group) => !value.includes(group.id))
  const items = selectable.map((group) => ({
    key: String(group.id),
    label: `${group.display_name} (${group.name})`,
  }))

  return (
    <div className="grid gap-2">
      <Select
        aria-label={t('channels.form.groupPlaceholder')}
        id={id}
        isDisabled={groupsQuery.isPending || groupsQuery.isError || value.length >= 64}
        isInvalid={invalid}
        items={items}
        placeholder={placeholder}
        selectedKeys={[]}
        size="sm"
        onSelectionChange={(keys) => {
          const idValue = Number(Array.from(keys)[0])
          if (Number.isSafeInteger(idValue) && !value.includes(idValue)) onChange([...value, idValue])
        }}
      >
        {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
      </Select>
      {value.length > 0 ? (
        <div className="flex flex-wrap gap-1.5">
          {value.map((groupId) => {
            const group = groupsById.get(groupId)
            const label = group ? group.display_name : `#${groupId}`
            return (
              <Chip
                key={groupId}
                classNames={{
                  base: 'h-7 max-w-52 shrink-0 !flex-nowrap bg-success/10 pl-2 text-success',
                  content: 'min-w-0 truncate whitespace-nowrap',
                  closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
                }}
                size="sm"
                title={group?.name}
                variant="flat"
                onClose={() => onChange(value.filter((idValue) => idValue !== groupId))}
              >
                {label}
              </Chip>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
