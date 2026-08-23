import { Eye, LoaderCircle, Plus, Send, Trash2 } from 'lucide-react'
import { useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
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
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('organizationConsole.invitations.batch.title')}</SheetTitle>
          <SheetDescription>{t('organizationConsole.invitations.batch.description', { organization: organizationName })}</SheetDescription>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-xs font-semibold">{t('organizationConsole.invitations.batch.rows')}</h3>
              <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('organizationConsole.invitations.batch.count', { count: rows.length })}</p>
            </div>
            <Button type="button" size="sm" variant="secondary" disabled={rows.length >= 200 || previewMutation.isPending || createMutation.isPending} onClick={() => changeRows([...rows, emptyRow()])}><Plus aria-hidden="true" />{t('organizationConsole.invitations.batch.add')}</Button>
          </div>
          <div className="mt-3 grid gap-2">
            {rows.map((row, index) => {
              const previewItem = previewCurrent?.items.find((item) => item.index === index)
              return (
                <div key={row.id} className="rounded-lg border border-[var(--hairline)] p-3">
                  <div className="grid gap-2 sm:grid-cols-[minmax(0,1.3fr)_minmax(7rem,0.7fr)_minmax(8rem,0.8fr)_auto]">
                    <Input aria-label={t('organizationConsole.invitations.fields.email')} type="email" placeholder="name@example.com" value={row.email} disabled={createMutation.isPending} onChange={(event) => changeRows(rows.map((item) => item.id === row.id ? { ...item, email: event.target.value } : item))} />
                    <Select aria-label={t('organizationConsole.members.fields.role')} value={row.role} disabled={createMutation.isPending} onChange={(event) => changeRows(rows.map((item) => item.id === row.id ? { ...item, role: event.target.value } : item))}>
                      {['admin', 'billing', 'member', 'viewer'].map((role) => <option key={role} value={role}>{t(`organizationConsole.roles.${role}`)}</option>)}
                    </Select>
                    <Select aria-label={t('organizationConsole.members.fields.team')} value={row.teamId ?? ''} disabled={createMutation.isPending} onChange={(event) => changeRows(rows.map((item) => item.id === row.id ? { ...item, teamId: Number(event.target.value) || undefined } : item))}>
                      <option value="">{t('organizationConsole.members.noTeam')}</option>
                      {teams.filter((team) => team.status === 'active').map((team) => <option key={team.id} value={team.id}>{team.name}</option>)}
                    </Select>
                    <Button type="button" size="icon" variant="ghost" disabled={rows.length === 1 || createMutation.isPending} aria-label={t('organizationConsole.invitations.batch.remove')} onClick={() => changeRows(rows.filter((item) => item.id !== row.id))}><Trash2 aria-hidden="true" /></Button>
                  </div>
                  {previewItem ? (
                    <div className="mt-2 flex flex-wrap items-center gap-2 text-xs">
                      <Badge className={previewItem.ready ? 'text-success' : 'text-destructive'}>{t(previewItem.ready ? 'organizationConsole.invitations.batch.ready' : 'organizationConsole.invitations.batch.blocked')}</Badge>
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
        </div>
        <SheetFooter className="border-t border-[var(--hairline)]">
          <Button type="button" variant="outline" disabled={previewMutation.isPending || createMutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationConsole.actions.cancel')}</Button>
          <Button type="button" variant="secondary" disabled={!valid || previewMutation.isPending || createMutation.isPending} onClick={() => void preview()}>{previewMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Eye aria-hidden="true" />}{t('organizationConsole.invitations.batch.preview')}</Button>
          <Button type="button" disabled={!previewCurrent?.ready || createMutation.isPending} onClick={() => void submit()}>{createMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Send aria-hidden="true" />}{t('organizationConsole.invitations.batch.submit')}</Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}
