import { useEffect, useMemo, useState } from 'react'
import { Check, RefreshCw, Search } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button, Chip, Input, Skeleton, Textarea } from '@heroui/react'
import { useAdminModelMetadata } from '@/features/model-management/model-management-api'
import { cn } from '@/lib/utils'
import { useAdminGroupModelPolicy, useUpdateAdminGroupModelPolicy } from './group-model-policy-api'

const MAX_MODELS = 512
const MAX_MODEL_NAME_BYTES = 256

/** 独立配置分组的模型访问白名单；不参与分组基础倍率或 Key 用户授权设置。 */
export function GroupModelPolicyEditor({ groupId }: { groupId: number }) {
  const { t } = useTranslation()
  const policyQuery = useAdminGroupModelPolicy(groupId)
  if (policyQuery.isPending) {
    return <p role="status" className="p-4 text-sm text-muted-foreground">{t('groups.modelPolicy.loading')}</p>
  }
  if (policyQuery.isError) {
    return (
      <div role="alert" className="m-4 grid gap-3 rounded-lg border border-destructive/25 p-4 text-sm text-destructive">
        <p>{t('groups.modelPolicy.loadError')}</p>
        <Button type="button" size="sm" variant="flat" onClick={() => void policyQuery.refetch()}>{t('groups.actions.retry')}</Button>
      </div>
    )
  }
  return <GroupModelPolicyForm key={groupId} groupId={groupId} allowedModels={policyQuery.data.allowed_models} />
}

function GroupModelPolicyForm({ groupId, allowedModels }: {
  groupId: number
  allowedModels: string[] | null
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState((allowedModels ?? []).join('\n'))
  const [search, setSearch] = useState('')
  const modelsQuery = useAdminModelMetadata(100)
  const mutation = useUpdateAdminGroupModelPolicy(groupId)
  useEffect(() => setDraft((allowedModels ?? []).join('\n')), [allowedModels])

  const selectedModels = useMemo(() => parseModelDraft(draft), [draft])
  const models = useMemo(
    () => modelsQuery.data?.pages.flatMap((page) => page.models).filter((model) => model.lifecycle !== 'retired') ?? [],
    [modelsQuery.data],
  )
  const filteredModels = useMemo(() => {
    const query = search.trim().toLocaleLowerCase()
    if (!query) return models
    return models.filter((model) => `${model.model} ${model.display_name} ${model.provider}`.toLocaleLowerCase().includes(query))
  }, [models, search])
  const validationError = validateModelDraft(draft)
  const invalidDraft = validationError !== undefined
  const canSave = !mutation.isPending && !invalidDraft

  const toggleModel = (model: string) => {
    const next = new Set(selectedModels)
    if (next.has(model)) next.delete(model)
    else next.add(model)
    setDraft([...next].sort((left, right) => left.localeCompare(right)).join('\n'))
    mutation.reset()
  }

  const save = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canSave) return
    mutation.mutate({ allowed_models: selectedModels.length > 0 ? selectedModels : null })
  }

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={save}>
      <div className="flex-1 space-y-4 overflow-y-auto px-4 py-5">
        <div>
          <h3 className="text-sm font-semibold">{t('groups.modelPolicy.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('groups.modelPolicy.description')}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Chip size="sm" variant="flat" className="bg-surface-2 text-muted-foreground">
            {selectedModels.length === 0
              ? t('groups.modelPolicy.unrestricted')
              : t('groups.modelPolicy.selectedCount', { count: selectedModels.length })}
          </Chip>
          <Button type="button" size="sm" variant="light" isDisabled={mutation.isPending || selectedModels.length === 0} onClick={() => { setDraft(''); mutation.reset() }}>
            {t('groups.modelPolicy.clear')}
          </Button>
        </div>
        <label className="grid gap-1.5" htmlFor="group-model-policy-search">
          <span className="text-xs font-medium">{t('groups.modelPolicy.catalogLabel')}</span>
          <Input id="group-model-policy-search" startContent={<Search className="size-3.5 text-muted-foreground" aria-hidden="true" />} value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t('groups.modelPolicy.searchPlaceholder')} />
        </label>
        {modelsQuery.isPending ? (
          <div className="grid gap-1.5" aria-label={t('groups.modelPolicy.loadingModels')}>
            {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-10 rounded-lg" />)}
          </div>
        ) : modelsQuery.isError ? (
          <div role="alert" className="rounded-lg border border-warning/20 bg-warning/8 p-3 text-xs text-muted-foreground">
            <p>{t('groups.modelPolicy.catalogError')}</p>
            <Button type="button" size="sm" variant="light" className="mt-1" onClick={() => void modelsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('groups.actions.retry')}</Button>
          </div>
        ) : (
          <div className="grid gap-1 rounded-lg border border-[var(--hairline)] p-1.5" role="listbox" aria-label={t('groups.modelPolicy.catalogLabel')} aria-multiselectable="true">
            {filteredModels.length === 0 ? <p className="p-2 text-xs text-muted-foreground">{t('groups.modelPolicy.noModels')}</p> : filteredModels.map((model) => {
              const selected = selectedModels.includes(model.model)
              return (
                <button
                  key={model.id}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  disabled={mutation.isPending || (!selected && selectedModels.length >= MAX_MODELS)}
                  className={cn('flex min-h-9 items-center gap-2 rounded-md px-2 text-left outline-none hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-50', selected && 'bg-surface-2')}
                  onClick={() => toggleModel(model.model)}
                >
                  <Check className={cn('size-3.5 shrink-0 text-info', !selected && 'opacity-0')} aria-hidden="true" />
                  <span className="min-w-0 flex-1 truncate font-mono text-xs">{model.model}</span>
                  <span className="max-w-32 truncate text-[0.6875rem] text-muted-foreground">{model.display_name}</span>
                </button>
              )
            })}
            {modelsQuery.hasNextPage ? (
              <Button type="button" size="sm" variant="light" className="justify-center" isDisabled={modelsQuery.isFetchingNextPage} onClick={() => void modelsQuery.fetchNextPage()}>
                {t(modelsQuery.isFetchingNextPage ? 'groups.modelPolicy.loadingMore' : 'groups.modelPolicy.loadMore')}
              </Button>
            ) : null}
          </div>
        )}
        <label className="grid gap-1.5" htmlFor="group-model-policy-custom">
          <span className="text-xs font-medium">{t('groups.modelPolicy.customLabel')}</span>
          <Textarea
            id="group-model-policy-custom"
            value={draft}
            isDisabled={mutation.isPending}
            isInvalid={invalidDraft}
            onChange={(event) => { setDraft(event.target.value); mutation.reset() }}
            placeholder={t('groups.modelPolicy.customPlaceholder')}
          />
          <span className={cn('text-xs leading-5', invalidDraft ? 'text-destructive' : 'text-muted-foreground')}>
            {validationError ? t(validationError) : t('groups.modelPolicy.customHint', { count: selectedModels.length, maximum: MAX_MODELS })}
          </span>
        </label>
        {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t('groups.modelPolicy.saveError')}</p> : null}
        {mutation.isSuccess ? <p role="status" className="text-xs text-success">{t('groups.modelPolicy.saved')}</p> : null}
      </div>
      <div className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] p-4">
        <p className="text-xs text-muted-foreground">{t('groups.modelPolicy.priorityHint')}</p>
        <Button type="submit" color="primary" isDisabled={!canSave}>{t(mutation.isPending ? 'groups.modelPolicy.saving' : 'groups.modelPolicy.save')}</Button>
      </div>
    </form>
  )
}

function parseModelDraft(draft: string): string[] {
  const models = draft.split(/[\r\n,]+/u).map((value) => value.trim()).filter(Boolean)
  return [...new Set(models)]
}

function validateModelDraft(draft: string): string | undefined {
  const rawModels = draft.split(/[\r\n,]+/u).filter((value) => value.length > 0)
  const models = parseModelDraft(draft)
  if (rawModels.some((value) => value.trim() !== value || new TextEncoder().encode(value).length > MAX_MODEL_NAME_BYTES || /\p{Cc}/u.test(value))) {
    return 'groups.modelPolicy.invalidName'
  }
  if (models.length > MAX_MODELS) return 'groups.modelPolicy.tooMany'
  if (models.reduce((total, model) => total + new TextEncoder().encode(model).length, 0) > 32 * 1024) return 'groups.modelPolicy.tooMuchText'
  return undefined
}
