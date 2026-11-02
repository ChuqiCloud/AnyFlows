import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Input, Select, SelectItem } from '@heroui/react'
import { Controller, useForm } from 'react-hook-form'
import { LoaderCircle, Route, UsersRound } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'

import type { IssuedUserToken, UserToken } from '@/lib/api/generated/types.gen'
import { hasManagementErrorCode, useCreateApiKey, useUpdateApiKey } from './api-key-api'
import { ApiKeyField, ApiKeySwitchField } from './api-key-form-field'
import {
  buildApiKeyFormSchema,
  defaultApiKeyValues,
  toApiKeyRequest,
  toCreateApiKeyRequest,
  type ApiKeyFormValues,
} from './api-key-form-model'
import { useUserTokenGroups } from './api-key-group-api'
import { isSelectableTokenGroup } from './api-key-group-model'
import { ApiKeyGroupSelector } from './api-key-group-selector'
import { isSelectableUserRoute, tokenUserRouteId } from '@/features/user-routes/user-route-model'
import { useUserRoutes } from '../user-routes/user-route-api'
import { ApiKeyIpEditor } from './api-key-ip-editor'
import { ApiKeyModelSelector } from './api-key-model-selector'

type ApiKeyFormProps = {
  token?: UserToken
  portalContainer?: HTMLElement | null
  onCancel: () => void
  onIssued: (issued: IssuedUserToken) => void
  onSaved: () => void
}

export function ApiKeyForm({ token, portalContainer, onCancel, onIssued, onSaved }: ApiKeyFormProps) {
  const { t } = useTranslation()
  const groupsQuery = useUserTokenGroups(!token)
  const createMutation = useCreateApiKey()
  const updateMutation = useUpdateApiKey()
  const form = useForm<ApiKeyFormValues>({
    defaultValues: defaultApiKeyValues(token),
    resolver: zodResolver(buildApiKeyFormSchema({
      invalidField: t('apiKeys.validation.field'),
      invalidModels: t('apiKeys.validation.models'),
      invalidIps: t('apiKeys.validation.ips'),
      invalidDate: t('apiKeys.validation.date'),
    })),
  })

  useEffect(() => form.reset(defaultApiKeyValues(token)), [form, token])

  const groupId = form.watch('groupId')
  const routingMode = form.watch('routingMode')
  const userRouteId = form.watch('userRouteId')
  const boundRouteId = token ? tokenUserRouteId(token) : undefined
  const routesQuery = useUserRoutes((!token && routingMode === 'route') || boundRouteId !== undefined)
  const unlimitedQuota = form.watch('unlimitedQuota')
  const groupSelectionReady = token
    ? true
    : routingMode === 'route'
      ? routesQuery.isSuccess && !routesQuery.isFetching && isSelectableUserRoute(userRouteId, routesQuery.data?.items)
      : groupsQuery.isSuccess && !groupsQuery.isFetching && isSelectableTokenGroup(groupId, groupsQuery.data)
  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const limitReached = hasManagementErrorCode(submitError, 'token_limit_reached')
  const groupUnavailable = hasManagementErrorCode(submitError, 'group_unavailable')
  const routeUnavailable = hasManagementErrorCode(submitError, 'user_route_unavailable') || hasManagementErrorCode(submitError, 'user_route_not_found')
  const changeRoutingMode = (mode: ApiKeyFormValues['routingMode']) => {
    form.setValue('routingMode', mode)
    form.clearErrors(['groupId', 'userRouteId'])
    createMutation.reset()
  }
  const onSubmit = form.handleSubmit(async (values) => {
    if (!token) {
      if (values.routingMode === 'route') {
        if (!routesQuery.isSuccess || routesQuery.isFetching || !isSelectableUserRoute(values.userRouteId, routesQuery.data?.items)) {
          form.setError('userRouteId', { message: t('apiKeys.routing.invalidRoute') })
          return
        }
      } else if (!groupsQuery.isSuccess || groupsQuery.isFetching || !isSelectableTokenGroup(values.groupId, groupsQuery.data)) {
        form.setError('groupId', { message: t('apiKeys.group.invalid') })
        return
      }
    }
    try {
      if (token) {
        await updateMutation.mutateAsync({ id: token.id, body: toApiKeyRequest(values) })
        onSaved()
      } else {
        const issued = await createMutation.mutateAsync(toCreateApiKeyRequest(values, groupsQuery.data ?? [], routesQuery.data?.items))
        createMutation.reset()
        onIssued(issued)
      }
    } catch (error) {
      if (!token && hasManagementErrorCode(error, 'group_unavailable')) {
        if (values.routingMode === 'route') {
          form.setError('userRouteId', { message: t('apiKeys.routing.unavailable') })
          void routesQuery.refetch()
        } else {
          form.setError('groupId', { message: t('apiKeys.group.unavailable') })
          void groupsQuery.refetch()
        }
      }
      if (!token && (hasManagementErrorCode(error, 'user_route_unavailable') || hasManagementErrorCode(error, 'user_route_not_found'))) {
        form.setError('userRouteId', { message: t('apiKeys.routing.unavailable') })
        void routesQuery.refetch()
      }
      // 保留表单输入；一次性密钥只有成功结果会交给临时展示面板。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="api-key-basic-fields">
          <h3 id="api-key-basic-fields" className="text-xs font-semibold">{t('apiKeys.form.basic')}</h3>
          <ApiKeyField id="api-key-name" label={t('apiKeys.fields.name')} error={form.formState.errors.name?.message}>
            <Input autoFocus id="api-key-name" isInvalid={!!form.formState.errors.name} size="sm" {...form.register('name')} />
          </ApiKeyField>
          {token && boundRouteId !== undefined ? (
            <ApiKeyField id="api-key-route-binding" label={t('apiKeys.routing.label')} hint={t('apiKeys.routing.immutable')}>
              <p id="api-key-route-binding" className="rounded-lg border border-[var(--hairline)] px-3 py-2 text-sm">
                {routesQuery.data?.items.find((route) => route.id === boundRouteId)?.name ?? `#${boundRouteId}`}
              </p>
              <Link className="text-xs text-primary hover:underline" to="/console/user-routes">{t('apiKeys.routing.manage')}</Link>
            </ApiKeyField>
          ) : null}
          {!token ? (
            <section className="grid gap-2">
              <span className="text-xs font-medium">{t('apiKeys.routing.label')}</span>
              <div role="group" aria-label={t('apiKeys.routing.label')} className="grid grid-cols-2 gap-1 rounded-lg bg-surface-2 p-1">
                <Button type="button" size="sm" variant={routingMode === 'group' ? 'solid' : 'light'} aria-pressed={routingMode === 'group'} isDisabled={pending} onPress={() => changeRoutingMode('group')}>
                  <UsersRound className="size-3.5" aria-hidden="true" />{t('apiKeys.routing.groupMode')}
                </Button>
                <Button type="button" size="sm" variant={routingMode === 'route' ? 'solid' : 'light'} aria-pressed={routingMode === 'route'} isDisabled={pending} onPress={() => changeRoutingMode('route')}>
                  <Route className="size-3.5" aria-hidden="true" />{t('apiKeys.routing.routeMode')}
                </Button>
              </div>
            </section>
          ) : null}
          {!token && routingMode === 'route' ? (
            <ApiKeyField id="api-key-user-route" label={t('apiKeys.routing.routeLabel')} error={form.formState.errors.userRouteId?.message ?? (!isSelectableUserRoute(userRouteId, routesQuery.data?.items) && routesQuery.isSuccess ? t('apiKeys.routing.invalidRoute') : undefined)} hint={t('apiKeys.routing.routeHint')}>
              <Select
                id="api-key-user-route"
                aria-label={t('apiKeys.routing.routeLabel')}
                placeholder={t('apiKeys.routing.routePlaceholder')}
                size="sm"
                selectedKeys={isSelectableUserRoute(userRouteId, routesQuery.data?.items) ? [userRouteId] : []}
                isDisabled={routesQuery.isFetching || routesQuery.isError || !routesQuery.data}
                isInvalid={!!form.formState.errors.userRouteId || (routesQuery.isSuccess && !isSelectableUserRoute(userRouteId, routesQuery.data.items))}
                popoverProps={portalContainer ? { portalContainer } : undefined}
                onSelectionChange={(keys) => { const value = Array.from(keys)[0]; if (value !== undefined) { form.setValue('userRouteId', String(value)); form.clearErrors('userRouteId') } }}
              >
                {(routesQuery.data?.items ?? []).filter((route) => route.enabled).map((route) => <SelectItem key={String(route.id)} textValue={route.name}>{route.name}</SelectItem>)}
              </Select>
              {routesQuery.isPending ? <p role="status" className="text-xs text-muted-foreground">{t('apiKeys.routing.loading')}</p> : null}
              {routesQuery.isError ? <div role="alert" className="flex items-center justify-between gap-2 text-xs text-destructive"><span>{t('apiKeys.routing.loadError')}</span><Button type="button" size="sm" variant="flat" isDisabled={routesQuery.isFetching} onPress={() => void routesQuery.refetch()}>{t('apiKeys.actions.retry')}</Button></div> : null}
              {routesQuery.isSuccess && routesQuery.data.items.filter((route) => route.enabled).length === 0 ? <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground"><span>{t('apiKeys.routing.empty')}</span><Link className="shrink-0 text-primary hover:underline" to="/console/user-routes">{t('apiKeys.routing.manage')}</Link></div> : <Link className="justify-self-start text-xs text-primary hover:underline" to="/console/user-routes">{t('apiKeys.routing.manage')}</Link>}
            </ApiKeyField>
          ) : null}
          {!token && routingMode === 'group' ? (
            <ApiKeyGroupSelector
              portalContainer={portalContainer}
              groups={groupsQuery.data}
              value={groupId}
              loading={groupsQuery.isFetching}
              failed={groupsQuery.isError}
              error={form.formState.errors.groupId?.message ?? (groupsQuery.isSuccess && !isSelectableTokenGroup(groupId, groupsQuery.data) ? t('apiKeys.group.invalid') : undefined)}
              onChange={(value) => { form.setValue('groupId', value); form.clearErrors('groupId') }}
              onRetry={() => void groupsQuery.refetch()}
            />
          ) : null}
          {token && boundRouteId === undefined ? (
            <ApiKeyGroupSelector token={token} portalContainer={portalContainer} groups={groupsQuery.data} value={groupId} loading={groupsQuery.isFetching} failed={groupsQuery.isError} onChange={() => undefined} onRetry={() => void groupsQuery.refetch()} />
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            <Controller
              control={form.control}
              name="status"
              render={({ field }) => (
                <ApiKeySwitchField
                  id="api-key-status"
                  label={t('apiKeys.fields.status')}
                  hint={t('apiKeys.form.statusHint')}
                  checked={field.value === 'enabled'}
                  onCheckedChange={(checked) => field.onChange(checked ? 'enabled' : 'disabled')}
                />
              )}
            />
            <ApiKeyField id="api-key-expiry" label={t('apiKeys.fields.expiredAt')} error={form.formState.errors.expiredAt?.message} hint={t('apiKeys.form.expiryHint')}>
              <Input id="api-key-expiry" isInvalid={!!form.formState.errors.expiredAt} size="sm" type="datetime-local" {...form.register('expiredAt')} />
            </ApiKeyField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="api-key-quota-fields">
          <h3 id="api-key-quota-fields" className="text-xs font-semibold">{t('apiKeys.form.quota')}</h3>
          <Controller
            control={form.control}
            name="unlimitedQuota"
            render={({ field }) => (
              <ApiKeySwitchField
                id="api-key-unlimited"
                label={t('apiKeys.fields.unlimitedQuota')}
                hint={t('apiKeys.form.unlimitedHint')}
                checked={field.value}
                onCheckedChange={field.onChange}
              />
            )}
          />
          <ApiKeyField id="api-key-quota" label={t('apiKeys.fields.remainQuota')} error={form.formState.errors.remainQuota?.message} hint={t('apiKeys.form.quotaHint')}>
            <Input id="api-key-quota" inputMode="numeric" isDisabled={unlimitedQuota} isInvalid={!!form.formState.errors.remainQuota} min={0} size="sm" type="number" {...form.register('remainQuota', { valueAsNumber: true })} />
          </ApiKeyField>
        </section>

        <section className="grid gap-3" aria-labelledby="api-key-policy-fields">
          <h3 id="api-key-policy-fields" className="text-xs font-semibold">{t('apiKeys.form.policy')}</h3>
          <ApiKeyField id="api-key-models" label={t('apiKeys.fields.modelLimits')} error={form.formState.errors.modelLimits?.message} hint={t('apiKeys.form.modelsHint')}>
            <Controller control={form.control} name="modelLimits" render={({ field }) => <ApiKeyModelSelector allowCustom={routingMode === 'route'} values={field.value} error={form.formState.errors.modelLimits?.message} onChange={field.onChange} />} />
          </ApiKeyField>
          <ApiKeyField id="api-key-ips" label={t('apiKeys.fields.allowIps')} error={form.formState.errors.allowIps?.message} hint={t('apiKeys.form.ipsHint')}>
            <Controller control={form.control} name="allowIps" render={({ field }) => <ApiKeyIpEditor values={field.value} error={form.formState.errors.allowIps?.message} onChange={field.onChange} />} />
          </ApiKeyField>
        </section>

        {submitError && (!groupUnavailable || !!token) && (!routeUnavailable || !!token) ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(groupUnavailable ? 'apiKeys.group.unavailable' : routeUnavailable ? 'apiKeys.routing.unavailable' : limitReached ? 'apiKeys.form.limitError' : 'apiKeys.form.submitError')}
          </p>
        ) : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" isDisabled={pending} onClick={onCancel}>{t('apiKeys.actions.cancel')}</Button>
        <Button color="primary" type="submit" isDisabled={pending || !groupSelectionReady}>
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
          {t(token ? 'apiKeys.actions.save' : 'apiKeys.actions.create')}
        </Button>
      </div>
    </form>
  )
}
