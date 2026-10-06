import { Button, Checkbox, Chip, Input, Select, SelectItem } from '@heroui/react'
import { useEffect, useMemo, useState } from 'react'
import { CheckCircle2, ListPlus, RefreshCw, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { modelSyncErrorCode, useImportMissingAdminModels, useMissingAdminModels } from './model-sync-api'
import { MissingModelImportSheet } from './missing-model-import-sheet'
import { toMissingModelImportRequest, type MissingModelImportDraft } from './missing-model-form-model'
import { MissingModelList } from './missing-model-list'

const MAX_IMPORT_ITEMS = 100

/** 管理渠道已声明但商品目录尚未登记的 Canonical 模型。 */
export function MissingModelWorkspace() {
  const { t } = useTranslation()
  const query = useMissingAdminModels()
  const mutation = useImportMissingAdminModels()
  const [search, setSearch] = useState('')
  const [channelId, setChannelId] = useState('all')
  const [selected, setSelected] = useState<Set<string>>(() => new Set())
  const [displayNames, setDisplayNames] = useState<Record<string, string>>({})
  const [sheetOpen, setSheetOpen] = useState(false)
  const [lastImportedCount, setLastImportedCount] = useState<number>()

  const models = useMemo(() => query.data?.pages.flatMap((page) => page.models) ?? [], [query.data])
  const channels = useMemo(() => {
    const values = new Map<number, string>()
    models.forEach((model) => model.channels.forEach((channel) => values.set(channel.channel_id, channel.channel_name)))
    return [...values].sort((left, right) => left[1].localeCompare(right[1]))
  }, [models])
  const filtered = useMemo(() => {
    const keyword = search.trim().toLocaleLowerCase()
    return models.filter((model) => {
      const matchesSearch = !keyword || model.model.toLocaleLowerCase().includes(keyword)
      const matchesChannel = channelId === 'all' || model.channels.some((channel) => String(channel.channel_id) === channelId)
      return matchesSearch && matchesChannel
    })
  }, [channelId, models, search])
  const selectedModels = useMemo(() => models.filter((model) => selected.has(model.model)), [models, selected])
  const allFilteredSelected = filtered.length > 0 && filtered.every((model) => selected.has(model.model))

  useEffect(() => {
    const available = new Set(models.map((model) => model.model))
    setSelected((current) => {
      const next = new Set([...current].filter((model) => available.has(model)))
      return next.size === current.size ? current : next
    })
  }, [models])

  const toggle = (model: string, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current)
      if (checked && next.size < MAX_IMPORT_ITEMS) next.add(model)
      if (!checked) next.delete(model)
      return next
    })
    if (checked && displayNames[model] === undefined) {
      setDisplayNames((current) => ({ ...current, [model]: model }))
    }
  }

  const toggleFiltered = (checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current)
      if (!checked) {
        filtered.forEach((model) => next.delete(model.model))
        return next
      }
      for (const model of filtered) {
        if (next.size >= MAX_IMPORT_ITEMS) break
        next.add(model.model)
      }
      return next
    })
    if (checked) {
      setDisplayNames((current) => {
        const next = { ...current }
        filtered.slice(0, MAX_IMPORT_ITEMS).forEach((model) => { next[model.model] ??= model.model })
        return next
      })
    }
  }

  const importModels = async (draft: MissingModelImportDraft) => {
    try {
      const result = await mutation.mutateAsync(toMissingModelImportRequest(selectedModels, displayNames, draft))
      setLastImportedCount(result.models.length)
      setSelected(new Set())
      setDisplayNames({})
      setSheetOpen(false)
    } catch {
      // 保留选择和填写内容，管理员可根据稳定冲突信息刷新后重试。
    }
  }

  const openImportSheet = () => {
    mutation.reset()
    setSheetOpen(true)
  }

  const errorCode = mutation.error ? modelSyncErrorCode(mutation.error) ?? 'unknown' : undefined
  // HeroUI Select 的动态选项必须走 items + 渲染函数。
  const channelItems = [
    { key: 'all', label: t('modelManagement.missing.allChannels') },
    ...channels.map(([id, name]) => ({ key: String(id), label: name })),
  ]

  return (
    <div className="grid gap-4">
      <header className="flex flex-col gap-3 border-b border-[var(--hairline)] pb-3 lg:flex-row lg:items-end lg:justify-between">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold">{t('modelManagement.missing.title')}</h3>
            <Chip size="sm" variant="flat">{t('modelManagement.missing.loaded', { count: models.length })}</Chip>
            <Chip size="sm" variant="flat">{t('modelManagement.missing.selected', { count: selected.size })}</Chip>
          </div>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('modelManagement.missing.description')}</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="light" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
            <RefreshCw className={cn('size-3.5', query.isFetching && 'animate-spin')} aria-hidden="true" />{t('modelManagement.actions.refresh')}
          </Button>
          <Button type="button" color="primary" size="sm" isDisabled={selected.size === 0} onClick={openImportSheet}>
            <ListPlus className="size-3.5" aria-hidden="true" />{t('modelManagement.missing.import.action', { count: selected.size })}
          </Button>
        </div>
      </header>

      {lastImportedCount !== undefined ? (
        <div role="status" className="flex items-start gap-2 rounded-lg border border-success/20 bg-success/8 p-3 text-xs text-success">
          <CheckCircle2 className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>{t('modelManagement.missing.import.success', { count: lastImportedCount })}</span>
        </div>
      ) : null}
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(12rem,0.35fr)]">
        <Input
          size="sm"
          startContent={<Search className="size-3.5 text-muted-foreground" aria-hidden="true" />}
          value={search}
          placeholder={t('modelManagement.missing.search')}
          onChange={(event) => setSearch(event.target.value)}
        />
        <Select
          aria-label={t('modelManagement.missing.channelFilter')}
          items={channelItems}
          selectedKeys={[channelId]}
          size="sm"
          onSelectionChange={(keys) => setChannelId(String(Array.from(keys)[0] ?? 'all'))}
        >
          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
        </Select>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-surface-2/55 px-3 py-2 text-xs">
        {/* HeroUI Checkbox 用 children 渲染内联标签，避免嵌套 label。 */}
        <Checkbox
          isSelected={allFilteredSelected}
          isDisabled={filtered.length === 0}
          size="sm"
          onValueChange={(value) => toggleFiltered(value === true)}
        >
          {t('modelManagement.missing.selectFiltered', { count: filtered.length })}
        </Checkbox>
        <span className="text-muted-foreground">{t('modelManagement.missing.limit', { count: MAX_IMPORT_ITEMS })}</span>
      </div>

      <MissingModelList
        error={query.isError && query.data === undefined}
        fetchMore={() => void query.fetchNextPage()}
        loading={query.isPending}
        loadingMore={query.isFetchingNextPage}
        models={filtered}
        moreError={query.isFetchNextPageError}
        refresh={() => void query.refetch()}
        selected={selected}
        selectionFull={selected.size >= MAX_IMPORT_ITEMS}
        showMore={Boolean(query.hasNextPage)}
        onToggle={toggle}
      />

      <MissingModelImportSheet
        displayNames={displayNames}
        errorCode={errorCode}
        models={selectedModels}
        open={sheetOpen}
        pending={mutation.isPending}
        onDisplayNameChange={(model, displayName) => setDisplayNames((current) => ({ ...current, [model]: displayName }))}
        onImport={(draft) => void importModels(draft)}
        onOpenChange={setSheetOpen}
      />
    </div>
  )
}
