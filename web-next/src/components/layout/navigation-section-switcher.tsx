import { Button, Dropdown, DropdownItem, DropdownMenu, DropdownTrigger } from '@heroui/react'
import { Icon } from '@iconify/react'

export type NavigationSectionOption = {
  key: string
  title: string
}

type NavigationSectionSwitcherProps = {
  ariaLabel: string
  sections: NavigationSectionOption[]
  value: string | null
  onChange: (key: string) => void
}

/** 侧栏标题右侧的板块切换器：工作台 / 企业管理 / 系统管理，点选即换侧栏内容。 */
export function NavigationSectionSwitcher({
  ariaLabel,
  sections,
  value,
  onChange,
}: NavigationSectionSwitcherProps) {
  if (sections.length === 0) {
    return null
  }

  const active = sections.find((section) => section.key === value)

  return (
    <Dropdown placement="bottom-end">
      <DropdownTrigger>
        <Button
          className="min-w-0 max-w-[7rem] px-2 text-default-500"
          endContent={<Icon aria-hidden="true" className="shrink-0" icon="solar:alt-arrow-down-linear" width={14} />}
          size="sm"
          variant="light"
        >
          <span className="truncate text-xs">{active?.title ?? sections[0]?.title ?? ''}</span>
        </Button>
      </DropdownTrigger>
      <DropdownMenu
        aria-label={ariaLabel}
        items={sections}
        selectedKeys={active ? new Set([active.key]) : new Set<string>()}
        selectionMode="single"
        onSelectionChange={(keys) => {
          const nextKey = Array.from(keys)[0]

          if (typeof nextKey === 'string') {
            onChange(nextKey)
          }
        }}
      >
        {(item) => <DropdownItem key={item.key}>{item.title}</DropdownItem>}
      </DropdownMenu>
    </Dropdown>
  )
}
