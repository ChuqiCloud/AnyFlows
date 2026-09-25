import { useId, useRef, useState, type Ref } from 'react'
import { Check, ChevronsUpDown, Network, Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button, Popover, PopoverContent, PopoverTrigger } from '@heroui/react'
import { cn } from '@/lib/utils'
import { resolveProviderLogo } from './model-logos'
import { filterProviders, providerCatalog, providerDisplayName, type ProviderOption } from './provider-catalog'
import { findProviderOption, useModelProviderOptions } from './provider-catalog-api'

export function ProviderLabel({ provider }: { provider: string }) {
  const catalogQuery = useModelProviderOptions()
  const option = findProviderOption(provider, catalogQuery.options)
  const configuredLogo = option?.logo?.trim()
  const logo = resolveProviderLogo(configuredLogo, provider, option?.name, ...(option?.aliases ?? []))
  const Icon = logo?.Icon ?? Network
  return <span className="inline-flex min-w-0 items-center gap-2">{configuredLogo && /^https?:\/\//i.test(configuredLogo) ? <img className="size-4 shrink-0 object-contain" src={configuredLogo} alt="" loading="lazy" /> : <Icon className="size-4 shrink-0" aria-hidden="true" />}<span className="truncate">{option?.name ?? providerDisplayName(provider)}</span></span>
}

type ProviderPickerProps = {
  id: string
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  ref?: Ref<HTMLButtonElement>
  disabled?: boolean
  invalid?: boolean
  allowCustom?: boolean
  options?: readonly ProviderOption[]
}

export function ProviderPicker({ id, value, onChange, onBlur, ref, disabled, invalid, allowCustom = true, options = providerCatalog }: ProviderPickerProps) {
  const { t } = useTranslation()
  const listId = useId()
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const catalogQuery = useModelProviderOptions()
  const availableOptions = options === providerCatalog ? catalogQuery.options : options
  const matches = filterProviders(query, availableOptions)
  const custom = query.trim()
  const hasExactMatch = matches.some((item) => [item.id, item.name, ...(item.aliases ?? [])]
    .some((name) => name.toLowerCase() === custom.toLowerCase()))
  const choices = [...matches.map((item) => ({ value: item.id, custom: false })),
    ...(allowCustom && custom && !hasExactMatch ? [{ value: custom, custom: true }] : [])]
  const selectedIndex = Math.min(active, Math.max(0, choices.length - 1))
  const choose = (next: string) => {
    onChange(next)
    setOpen(false)
    onBlur?.()
  }
  return (
    <Popover placement="bottom-start" isOpen={open} onOpenChange={(next) => { setOpen(next); setQuery(''); setActive(0) }}>
      <PopoverTrigger>
        <Button ref={ref} id={id} type="button" variant="bordered" isDisabled={disabled} aria-invalid={invalid} onBlur={onBlur} className="h-9 w-full min-w-0 justify-between font-normal">
          {value ? <ProviderLabel provider={value} /> : <span className="truncate text-muted-foreground">{t('providers.select')}</span>}
          <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-72 max-w-[calc(100vw-2rem)] rounded-lg p-2">
        <input autoFocus ref={inputRef} role="combobox" aria-label={t('providers.search')} aria-expanded={open} aria-controls={listId} aria-autocomplete="list" aria-activedescendant={choices.length ? `${listId}-${selectedIndex}` : undefined} autoComplete="off" value={query} placeholder={t(allowCustom ? 'providers.searchOrCreate' : 'providers.search')} className="mb-1 h-9 w-full rounded-md border border-[var(--hairline)] bg-surface-1 px-2 text-sm outline-none focus:border-primary" onChange={(event) => { setQuery(event.target.value); setActive(0) }} onKeyDown={(event) => {
          if (event.nativeEvent.isComposing) return
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            const next = (selectedIndex + (event.key === 'ArrowDown' ? 1 : -1) + choices.length) % Math.max(choices.length, 1)
            setActive(next)
            document.getElementById(`${listId}-${next}`)?.scrollIntoView({ block: 'nearest' })
          } else if (event.key === 'Enter') {
            event.preventDefault()
            if (choices[selectedIndex]) choose(choices[selectedIndex].value)
          }
        }} />
        <div id={listId} role="listbox" aria-label={t('providers.select')} className="max-h-64 overflow-y-auto overscroll-contain">
          {choices.map((choice, index) => (
            <button key={choice.value} id={`${listId}-${index}`} type="button" role="option" aria-selected={value === choice.value} tabIndex={-1} className={cn('flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-2 text-left text-sm', index === selectedIndex && 'bg-accent text-accent-foreground')} onMouseEnter={() => setActive(index)} onMouseDown={(event) => event.preventDefault()} onClick={() => choose(choice.value)}>
              {choice.custom ? <><Plus className="size-4 shrink-0" aria-hidden="true" /><span className="break-all">{t('providers.create', { name: choice.value })}</span></> : <ProviderLabel provider={choice.value} />}
              {value === choice.value ? <Check className="ml-auto size-4 shrink-0" aria-hidden="true" /> : null}
            </button>
          ))}
          {!choices.length ? <p className="p-3 text-sm text-muted-foreground">{t('providers.empty')}</p> : null}
          {allowCustom && !custom ? <p className="border-t px-2 py-2 text-xs text-muted-foreground">{t('providers.searchOrCreate')}</p> : null}
        </div>
      </PopoverContent>
    </Popover>
  )
}
