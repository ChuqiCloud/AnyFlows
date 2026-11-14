import { Button, Chip, Input, Skeleton } from '@heroui/react'
import { BookOpen } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { usePluginCatalogModels, usePluginCatalogSources } from './plugin-lifecycle-api'

/** User-facing model discovery. Plugin records never become platform price cards. */
export function PluginCatalogPanel() {
  const { t } = useTranslation()
  const sources = usePluginCatalogSources()
  const sourceList = sources.data?.pages.flatMap((page) => page.sources) ?? []
  const [pluginId, setPluginId] = useState('')
  const [searchInput, setSearchInput] = useState('')
  const [search, setSearch] = useState('')
  const selected = sourceList.find((source) => source.pluginId === pluginId)
  const models = usePluginCatalogModels(selected?.pluginId, search)
  const entries = models.data?.pages.flatMap((page) => page.models) ?? []

  useEffect(() => {
    const timer = window.setTimeout(() => setSearch(searchInput.trim()), 600)
    return () => window.clearTimeout(timer)
  }, [searchInput])

  return <section className="rounded-xl border border-[var(--hairline)] bg-card p-4 sm:p-5">
    <h2 className="flex items-center gap-2 text-base font-semibold"><BookOpen size={17} />{t('pluginCatalog.title')}</h2>
    <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t('pluginCatalog.description')}</p>
    {sources.isPending ? <Skeleton className="mt-4 h-10 rounded-lg" /> : sources.isError ? <div role="alert" className="mt-4 flex flex-wrap items-center justify-between gap-2 text-sm text-danger"><span>{t('pluginCatalog.sourceError')}</span><Button size="sm" variant="bordered" onPress={() => void sources.refetch()}>{t('pluginCatalog.retry')}</Button></div> : <>
      <div className="mt-4 flex flex-wrap items-end gap-3">
        <label className="grid min-w-48 flex-1 gap-1 text-xs font-medium">{t('pluginCatalog.source')}<select className="h-10 w-full rounded-lg border border-[var(--hairline)] bg-background px-3 text-sm" value={pluginId} onChange={(event) => { setPluginId(event.target.value); setSearchInput(''); setSearch('') }}><option value="">{t('pluginCatalog.chooseSource')}</option>{sourceList.map((source) => <option key={source.pluginId} value={source.pluginId}>{source.displayName} · {source.pluginVersion}</option>)}</select></label>
        {selected?.supportsSearch ? <Input className="min-w-48 flex-1" label={t('pluginCatalog.search')} maxLength={128} value={searchInput} onValueChange={setSearchInput} size="sm" type="search" /> : null}
        {selected ? <Button size="sm" variant="bordered" isDisabled={models.isFetching} onPress={() => void models.refetch()}>{t('pluginCatalog.refresh')}</Button> : null}
      </div>
      {sources.hasNextPage ? <Button className="mt-2" size="sm" variant="light" isDisabled={sources.isFetchingNextPage} onPress={() => void sources.fetchNextPage()}>{t('pluginCatalog.moreSources')}</Button> : null}
      {sourceList.length === 0 ? <p className="mt-4 text-sm text-muted-foreground">{t('pluginCatalog.noSources')}</p> : null}
      {selected ? <div className="mt-4">
        <p className="mb-3 text-xs text-muted-foreground">{t('pluginCatalog.sourceLabel', { source: selected.displayName, generation: selected.generation })}</p>
        {models.isPending ? <Skeleton className="h-28 rounded-lg" /> : models.isError ? <div role="alert" className="rounded-lg border border-danger/30 p-3 text-sm text-danger">{t('pluginCatalog.modelsError')}</div> : entries.length ? <div className="grid gap-3 sm:grid-cols-2">{entries.map((model) => <article key={model.id} className="min-w-0 rounded-lg border border-[var(--hairline)] p-4"><div className="flex items-start justify-between gap-2"><h3 className="break-words text-sm font-semibold">{model.displayName}</h3><Chip size="sm" variant="flat">{model.provider}</Chip></div><p className="mt-1 break-all font-mono text-xs text-muted-foreground">{model.id}</p>{model.description ? <p className="mt-2 whitespace-pre-wrap text-xs leading-5 text-muted-foreground">{model.description}</p> : null}<div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">{model.contextWindow ? <span>{t('pluginCatalog.context', { count: model.contextWindow })}</span> : null}{model.tags.map((tag) => <Chip key={tag} size="sm" variant="flat">{tag}</Chip>)}</div></article>)}</div> : <p className="text-sm text-muted-foreground">{t('pluginCatalog.empty')}</p>}
        {models.isFetchNextPageError ? <p role="alert" className="mt-3 text-xs text-danger">{t('pluginCatalog.modelsError')}</p> : null}
        {models.hasNextPage ? <Button className="mt-4" size="sm" variant="bordered" isDisabled={models.isFetchingNextPage} onPress={() => void models.fetchNextPage()}>{t('pluginCatalog.moreModels')}</Button> : null}
      </div> : null}
    </>}
  </section>
}
