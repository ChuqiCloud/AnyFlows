import { useState, type FormEvent } from 'react'
import { Button, Input, Select, SelectItem, Switch } from '@heroui/react'
import { ArrowDown, ArrowUp, LoaderCircle, Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ApiKeyField } from '@/features/api-keys/api-key-form-field'
import { useUserTokenGroups } from '@/features/api-keys/api-key-group-api'
import { formatGroupBaseRatio } from '@/features/api-keys/api-key-group-model'
import { useSaveUserRoute, userRouteErrorCode } from './user-route-api'
import {
  defaultUserRouteDraft, MAX_USER_ROUTE_MEMBERS, toUserRouteRequest, userRouteStrategies,
  validateUserRouteDraft, type UserRoute, type UserRouteDraft, type UserRouteMemberDraft, type UserRouteStrategy,
} from './user-route-model'

type Props = { route?: UserRoute; portalContainer?: HTMLElement | null; onCancel: () => void; onSaved: () => void }

export function UserRouteForm({ route, portalContainer, onCancel, onSaved }: Props) {
  const { t } = useTranslation()
  const groupsQuery = useUserTokenGroups()
  const mutation = useSaveUserRoute()
  const [draft, setDraft] = useState(() => defaultUserRouteDraft(route))
  const [validationError, setValidationError] = useState<string>()
  const groups = groupsQuery.data ?? []
  const pending = mutation.isPending
  const portal = portalContainer ? { portalContainer } : undefined
  const patch = (values: Partial<UserRouteDraft>) => {
    setDraft((current) => ({ ...current, ...values }))
    setValidationError(undefined)
    mutation.reset()
  }
  const patchMember = (rowId: string, values: Partial<UserRouteMemberDraft>) => {
    patch({ members: draft.members.map((member) => member.rowId === rowId ? { ...member, ...values } : member) })
  }
  const moveMember = (index: number, offset: number) => {
    const members = [...draft.members]
    const next = index + offset
    if (next < 0 || next >= members.length) return
    ;[members[index], members[next]] = [members[next], members[index]]
    patch({ members: members.map((member, priority) => ({ ...member, priority: String(priority) })) })
  }
  const addMember = () => {
    const next = groups.find((group) => !draft.members.some((member) => member.groupId === String(group.id)))
    if (!next) return
    patch({ members: [...draft.members, { rowId: crypto.randomUUID(), groupId: String(next.id), priority: String(draft.members.length), weight: '1' }] })
  }
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!groupsQuery.isSuccess || groupsQuery.isFetching) { setValidationError('groups'); return }
    const error = validateUserRouteDraft(draft, groups)
    if (error) { setValidationError(error); return }
    try {
      await mutation.mutateAsync({ id: route?.id, body: toUserRouteRequest(draft, groups) })
      onSaved()
    } catch {
      // Keep the current draft so permission changes can be corrected and retried.
      void groupsQuery.refetch()
    }
  }
  const serverError = userRouteErrorCode(mutation.error)

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pt-4 pb-5">
        <ApiKeyField id="user-route-name" label={t('userRoutes.fields.name')}>
          <Input autoFocus id="user-route-name" size="sm" value={draft.name} isDisabled={pending} onValueChange={(name) => patch({ name })} />
        </ApiKeyField>
        <div className="grid gap-4 sm:grid-cols-2">
          <ApiKeyField id="user-route-strategy" label={t('userRoutes.fields.strategy')}>
            <Select id="user-route-strategy" aria-label={t('userRoutes.fields.strategy')} size="sm" selectedKeys={[draft.strategy]} disallowEmptySelection isDisabled={pending} popoverProps={portal} onSelectionChange={(keys) => {
              const value = Array.from(keys)[0]
              if (typeof value === 'string' && userRouteStrategies.includes(value as UserRouteStrategy)) patch({ strategy: value as UserRouteStrategy })
            }}>
              {userRouteStrategies.map((strategy) => <SelectItem key={strategy}>{t(`userRoutes.strategies.${strategy}`)}</SelectItem>)}
            </Select>
          </ApiKeyField>
          <ApiKeyField id="user-route-enabled" label={t('userRoutes.fields.status')}>
            <Switch id="user-route-enabled" size="sm" isSelected={draft.enabled} isDisabled={pending} onValueChange={(enabled) => patch({ enabled })}>{t(draft.enabled ? 'userRoutes.enabled' : 'userRoutes.disabled')}</Switch>
          </ApiKeyField>
        </div>
        <p className="rounded-lg bg-surface-2 px-3 py-2 text-xs leading-5 text-muted-foreground">{t(`userRoutes.strategyHints.${draft.strategy}`)}</p>

        <section className="grid gap-3" aria-labelledby="user-route-members-title">
          <div>
            <h3 id="user-route-members-title" className="text-sm font-medium">{t('userRoutes.fields.members')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('userRoutes.membersHint')}</p>
          </div>
          {groupsQuery.isError ? (
            <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive"><span>{t('userRoutes.groupLoadError')}</span><Button type="button" size="sm" variant="flat" isDisabled={groupsQuery.isFetching} onPress={() => void groupsQuery.refetch()}>{t('userRoutes.actions.retry')}</Button></div>
          ) : null}
          {groupsQuery.isPending ? <p role="status" className="text-xs text-muted-foreground">{t('userRoutes.loadingGroups')}</p> : null}
          {draft.members.map((member, index) => {
            const authorized = groups.some((group) => String(group.id) === member.groupId)
            const options = [
              ...(!authorized ? [{ key: member.groupId, label: t('userRoutes.unavailableGroup', { id: member.groupId }), unavailable: true }] : []),
              ...groups.map((group) => ({
                key: String(group.id), unavailable: draft.members.some((other) => other.rowId !== member.rowId && other.groupId === String(group.id)),
                label: t('apiKeys.group.option', { name: group.display_name, ratio: formatGroupBaseRatio(group.ratio_micros) }),
              })),
            ]
            return (
              <div key={member.rowId} className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="text-xs font-medium">{t('userRoutes.memberNumber', { number: index + 1 })}</span>
                  <div className="flex gap-1">
                    <Button type="button" isIconOnly size="sm" variant="light" aria-label={t('userRoutes.actions.moveUp')} isDisabled={pending || index === 0} onPress={() => moveMember(index, -1)}><ArrowUp className="size-3.5" aria-hidden="true" /></Button>
                    <Button type="button" isIconOnly size="sm" variant="light" aria-label={t('userRoutes.actions.moveDown')} isDisabled={pending || index === draft.members.length - 1} onPress={() => moveMember(index, 1)}><ArrowDown className="size-3.5" aria-hidden="true" /></Button>
                    <Button type="button" isIconOnly size="sm" variant="light" color="danger" aria-label={t('userRoutes.actions.removeMember')} isDisabled={pending} onPress={() => patch({ members: draft.members.filter((other) => other.rowId !== member.rowId) })}><Trash2 className="size-3.5" aria-hidden="true" /></Button>
                  </div>
                </div>
                <Select aria-label={t('userRoutes.memberGroup', { number: index + 1 })} selectedKeys={[member.groupId]} size="sm" items={options} isDisabled={pending || !groupsQuery.isSuccess || groupsQuery.isFetching} isInvalid={groupsQuery.isSuccess && !authorized} popoverProps={portal} disallowEmptySelection onSelectionChange={(keys) => {
                  const value = Array.from(keys)[0]
                  if (value !== undefined) patchMember(member.rowId, { groupId: String(value) })
                }}>{(option) => <SelectItem key={option.key} isDisabled={option.unavailable}>{option.label}</SelectItem>}</Select>
                <div className="grid grid-cols-2 gap-3">
                  <Input aria-label={t('userRoutes.memberPriority', { number: index + 1 })} label={t('userRoutes.fields.priority')} size="sm" type="number" min={0} step={1} value={member.priority} isDisabled={pending} onValueChange={(priority) => patchMember(member.rowId, { priority })} />
                  <Input aria-label={t('userRoutes.memberWeight', { number: index + 1 })} label={t('userRoutes.fields.weight')} size="sm" type="number" min={1} max={10_000} step={1} value={member.weight} isDisabled={pending} onValueChange={(weight) => patchMember(member.rowId, { weight })} />
                </div>
              </div>
            )
          })}
          <Button type="button" size="sm" variant="flat" className="justify-self-start" isDisabled={pending || !groupsQuery.isSuccess || groupsQuery.isFetching || draft.members.length >= MAX_USER_ROUTE_MEMBERS || !groups.some((group) => !draft.members.some((member) => Number(member.groupId) === group.id))} onPress={addMember}><Plus className="size-3.5" aria-hidden="true" />{t('userRoutes.actions.addMember')}</Button>
          {groupsQuery.isSuccess && groups.length === 0 ? <p className="text-xs text-muted-foreground">{t('userRoutes.noGroups')}</p> : null}
        </section>

        <div className="grid gap-4 sm:grid-cols-2">
          <ApiKeyField id="user-route-attempts" label={t('userRoutes.fields.attempts')} hint={t('userRoutes.attemptsHint')}>
            <Input id="user-route-attempts" size="sm" type="number" min={1} max={16} step={1} value={draft.maxGroupAttempts} isDisabled={pending} onValueChange={(maxGroupAttempts) => patch({ maxGroupAttempts })} />
          </ApiKeyField>
          <ApiKeyField id="user-route-price" label={t('userRoutes.fields.maxPrice')} hint={t('userRoutes.priceHint')}>
            <Input id="user-route-price" size="sm" type="number" min={0} step="0.000001" inputMode="decimal" placeholder={t('userRoutes.unlimitedPrice')} value={draft.maxPriceRatio} isDisabled={pending} onValueChange={(maxPriceRatio) => patch({ maxPriceRatio })} />
          </ApiKeyField>
        </div>
        {validationError ? <p role="alert" className="rounded-lg bg-destructive/10 p-3 text-xs text-destructive">{t(`userRoutes.validation.${validationError}`)}</p> : null}
        {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/10 p-3 text-xs text-destructive">{t(`userRoutes.errors.${serverError ?? 'unknown'}`, { defaultValue: t('userRoutes.errors.unknown') })}</p> : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" isDisabled={pending} onPress={onCancel}>{t('userRoutes.actions.cancel')}</Button>
        <Button type="submit" color="primary" isDisabled={pending || !groupsQuery.isSuccess || groupsQuery.isFetching}>
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t(route ? 'userRoutes.actions.save' : 'userRoutes.actions.create')}
        </Button>
      </div>
    </form>
  )
}
