import { Button, Chip, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Input, Select, SelectItem } from '@heroui/react'
import { Eye, LoaderCircle, Plus, Send, Trash2 } from 'lucide-react'
import { useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { createTopupIdempotencyKey } from '@/features/wallet/topup-form-model'
import type { OrganizationInvitationCreateRequest, OrganizationTeamResponse } from '@/lib/api/generated/types.gen'
import { organizationMemberErrorCode, useCreateOrganizationInvitationBatch, usePreviewOrganizationInvitations } from './organization-member-api'
import { invitationBody } from './organization-invitation-editor-sheet'

type InvitationRow = {
  id: string
  email: string
  role: string
  teamId?: number
}

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"不加入团队"。 */
const NO_TEAM_KEY = '__none__'
const BATCH_ROLES = ['admin', 'billing', 'member', 'viewer'] as const

function emptyRow(): InvitationRow {
  return { id: createTopupIdempotencyKey(), email: '', role: 'member' }
}

export function OrganizationBatchInvitationSheet({
  organizationId,
  organizationName,
  teams,
  open,
  onOpenChange,
}: {
  organizationId: number
  organizationName: string
  teams: OrganizationTeamResponse[]
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const previewMutation = usePreviewOrganizationInvitations(organizationId)
  const createMutation = useCreateOrganizationInvitationBatch(organizationId)
  const [rows, setRows] = useState<InvitationRow[]>([emptyRow()])
  const batchKey = useRef(createTopupIdempotencyKey())
  const previewFingerprint = useRef<string | undefined>(undefined)
  const items = useMemo(
    () => rows.map((row) => invitationBody(row.email, row.role, row.teamId)).filter((item): item is OrganizationInvitationCreateRequest => item !== undefined),
    [rows],
  )
  const fingerprint = JSON.stringify(items)
  const previewCurrent = previewFingerprint.current === fingerprint ? previewMutation.data : undefined
  const valid = items.length === rows.length && items.length > 0 && items.length <= 200
  const teamItems = [
    { key: NO_TEAM_KEY, label: t('organizationConsole.members.noTeam') },
    ...teams.filter((team) => team.status === 'active').map((team) => ({ key: String(team.id), label: team.name })),
  ]

  const changeRows = (next: InvitationRow[]) => {
    setRows(next)
    batchKey.current = createTopupIdempotencyKey()
    previewFingerprint.current = undefined
    previewMutation.reset()
    createMutation.reset()
  }
  const preview = async () => {
    if (!valid) return
    const result = await previewMutation.mutateAsync({ items })
    previewFingerprint.current = fingerprint
    return result
  }
  const submit = async () => {
    if (!previewCurrent?.ready) return
    try {
      await createMutation.mutateAsync({ batch_key: batchKey.current, items })
      setRows([emptyRow()])
      previewFingerprint.current = undefined
      onOpenChange(false)
    } catch {
      // 相同批次失败重试继续复用批次键，避免重复创建邀请。
    }
  }

  return (
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-2xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('organizationConsole.invitations.batch.title')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationConsole.invitations.batch.description', { organization: organizationName })}</p>
            </DrawerHeader>
            <DrawerBody className="gap-0 p-4">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <h3 className="text-xs font-semibold">{t('organizationConsole.invitations.batch.rows')}</h3>
                  <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('organizationConsole.invitations.batch.count', { count: rows.length })}</p>
                </div>
                <Button type="button" size="sm" variant="bordered" isDisabled={rows.length >= 200 || previewMutation.isPending || createMutation.isPending} onClick={() => changeRows([...rows, emptyRow()])}><Plus className="size-3.5" aria-hidden="true" />{t('organizationConsole.invitations.batch.add')}</Button>
              </div>
              <div className="mt-3 grid gap-2">
                {rows.map((row, index) => {
                  const previewItem = previewCurrent?.items.find((item) => item.index === index)
                  return (
                    <div key={row.id} className="rounded-lg border border-[var(--hairline)] p-3">
                      <div className="grid gap-2 sm:grid-cols-[minmax(0,1.3fr)_minmax(7rem,0.7fr)_minmax(8rem,0.8fr)_auto]">
                        <Input aria-label={t('organizationConsole.invitations.fields.email')} type="email" placeholder="name@example.com" value={row.email} isDisabled={createMutation.isPending} size="sm" onChange={(event) => changeRows(rows.map((item) => item.id === row.id ? { ...item, email: event.target.value } : item))} />
                        <Select aria-label={t('organizationConsole.members.fields.role')} selectedKeys={[row.role]} isDisabled={createMutation.isPending} size="sm" onSelectionChange={(keys) => changeRows(rows.map((item) => item.id === row.id ? { ...item, role: String(Array.from(keys)[0] ?? 'member') } : item))}>
                          {BATCH_ROLES.map((role) => <SelectItem key={role}>{t(`organizationConsole.roles.${role}`)}</SelectItem>)}
                        </Select>
                        <Select
                          aria-label={t('organizationConsole.members.fields.team')}
                          isDisabled={createMutation.isPending}
                          items={teamItems}
                          selectedKeys={[row.teamId === undefined ? NO_TEAM_KEY : String(row.teamId)]}
                          size="sm"
                          onSelectionChange={(keys) => {
                            const next = String(Array.from(keys)[0] ?? NO_TEAM_KEY)
                            changeRows(rows.map((item) => item.id === row.id ? { ...item, teamId: next === NO_TEAM_KEY ? undefined : Number(next) } : item))
                          }}
                        >
                          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                        </Select>
                        <Button type="button" isIconOnly size="md" variant="light" isDisabled={rows.length === 1 || createMutation.isPending} aria-label={t('organizationConsole.invitations.batch.remove')} onClick={() => changeRows(rows.filter((item) => item.id !== row.id))}><Trash2 className="size-4" aria-hidden="true" /></Button>
                      </div>
                      {previewItem ? (
                        <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                          <Chip className={previewItem.ready ? 'text-success' : 'text-destructive'} size="sm" variant="flat">{t(previewItem.ready ? 'organizationConsole.invitations.batch.ready' : 'organizationConsole.invitations.batch.blocked')}</Chip>
                          {previewItem.issues.map((issue) => <span key={issue} className="text-muted-foreground">{t(`organizationConsole.invitations.issues.${issue}`, { defaultValue: issue })}</span>)}
                        </div>
                      ) : null}
                    </div>
                  )
                })}
              </div>
              {!valid ? <p className="mt-3 text-xs text-destructive">{t('organizationConsole.invitations.batch.validation')}</p> : null}
              {previewCurrent ? <p className={`mt-3 rounded-lg px-3 py-2 text-xs ${previewCurrent.ready ? 'bg-success/8 text-success' : 'bg-warning/8 text-warning'}`}>{t(previewCurrent.ready ? 'organizationConsole.invitations.batch.previewReady' : 'organizationConsole.invitations.batch.previewBlocked', { count: previewCurrent.available_seats })}</p> : null}
              {(previewMutation.isError || createMutation.isError) ? <p role="alert" className="mt-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.errors.${organizationMemberErrorCode(previewMutation.error ?? createMutation.error) === 'organization_conflict' ? 'conflict' : 'mutation'}`)}</p> : null}
            </DrawerBody>
            <DrawerFooter className="border-t border-[var(--hairline)]">
              <Button type="button" variant="bordered" isDisabled={previewMutation.isPending || createMutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
              <Button type="button" variant="bordered" isDisabled={!valid || previewMutation.isPending || createMutation.isPending} onClick={() => void preview()}>{previewMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Eye className="size-4" aria-hidden="true" />}{t('organizationConsole.invitations.batch.preview')}</Button>
              <Button type="button" color="primary" isDisabled={!previewCurrent?.ready || createMutation.isPending} onClick={() => void submit()}>{createMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Send className="size-4" aria-hidden="true" />}{t('organizationConsole.invitations.batch.submit')}</Button>
            </DrawerFooter>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}
