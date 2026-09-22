import { Button, Card, CardBody, Input } from '@heroui/react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Building2 } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { organizationDirectoryQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient } from '@/lib/api'
import { acceptOrganizationInvitation } from '@/lib/api/generated/sdk.gen'
import { cn } from '@/lib/utils'

const tokenPattern = /^oinv_[^.\s]+\.[A-Za-z0-9_-]{43}$/

/** 接受企业邀请后刷新工作空间目录，服务端仍按登录邮箱和令牌摘要复验。 */
function useAcceptOrganizationInvitation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (token: string) => {
      const { data } = await acceptOrganizationInvitation({ body: { token }, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: organizationDirectoryQueryKey }),
  })
}

function readInvitationTokenFromUrl() {
  return new URLSearchParams(window.location.search).get('organization_token') ?? ''
}

/** 粘贴管理员签发的一次性令牌加入企业；compact 模式用于已有企业时的次要入口。 */
export function OrganizationInvitationAcceptCard({ compact = false }: { compact?: boolean }) {
  const { t } = useTranslation()
  const mutation = useAcceptOrganizationInvitation()
  const [token, setToken] = useState(readInvitationTokenFromUrl)
  const valid = tokenPattern.test(token)

  const submit = async () => {
    if (!valid) return
    try {
      await mutation.mutateAsync(token)
      setToken('')
    } catch {
      // 失败时保留令牌，用户可核对登录邮箱或按原令牌重试。
    }
  }

  return (
    <Card className="border border-[var(--hairline)]" shadow="none">
      <CardBody className={cn('gap-4', compact ? 'p-4 lg:flex-row lg:items-center' : 'p-5')}>
        <div className="flex min-w-0 flex-1 items-start gap-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-full bg-brand/10 text-brand">
            <Building2 className="size-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h3 className={cn('font-semibold', compact ? 'text-sm' : 'text-base')}>{t('organizationConsole.join.title')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.join.subtitle')}</p>
          </div>
        </div>
        <div className={cn('w-full', compact && 'lg:max-w-xl')}>
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
            <Input
              aria-label={t('organizationConsole.join.token')}
              autoComplete="off"
              classNames={{ input: 'font-mono text-xs' }}
              isDisabled={mutation.isPending}
              placeholder="oinv_…"
              size="sm"
              value={token}
              onChange={(event) => { setToken(event.target.value.trim()); mutation.reset() }}
            />
            <Button type="button" color="primary" isDisabled={!valid || mutation.isPending} onClick={() => void submit()}>
              {t('organizationConsole.join.accept')}
            </Button>
          </div>
          {token && !valid ? <p className="mt-2 text-xs text-destructive">{t('organizationConsole.join.invalid')}</p> : null}
          {mutation.isSuccess ? (
            <p role="status" className="mt-2 rounded-lg bg-success/8 px-3 py-2 text-xs text-success">
              {t('organizationConsole.join.accepted', { organizationId: mutation.data.organization_id })}
            </p>
          ) : null}
          {mutation.isError ? (
            <p role="alert" className="mt-2 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
              {t('organizationConsole.join.error')}
            </p>
          ) : null}
        </div>
      </CardBody>
    </Card>
  )
}
