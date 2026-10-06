import { Check, LoaderCircle, Share2, ShieldCheck } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { copyText } from '@/lib/clipboard'
import type { PlaygroundShareCreateResponse } from '@/lib/api/generated/types.gen'
import { useCreatePlaygroundShare, useRevokePlaygroundShare } from './playground-share-api'
import { classifyShareMutationError } from './playground-share-errors'
import { PlaygroundShareIssued } from './playground-share-issued'
import { buildPlaygroundShareSnapshot } from './playground-share-snapshot'
import type { PlaygroundSession } from './playground-types'

type PlaygroundShareControlProps = {
  sessions: PlaygroundSession[]
  streaming: boolean
}

function useShareExpired(expiresAt?: number) {
  const [expired, setExpired] = useState(false)

  useEffect(() => {
    setExpired(false)
    if (!expiresAt) return
    let timeout: number | undefined
    const schedule = () => {
      const remaining = expiresAt * 1000 - Date.now()
      if (remaining <= 0) {
        setExpired(true)
        return
      }
      timeout = window.setTimeout(schedule, Math.min(remaining + 50, 2_147_483_647))
    }
    schedule()
    return () => window.clearTimeout(timeout)
  }, [expiresAt])

  return expired
}

function currentShareUrl(token: string) {
  const url = new URL(window.location.href)
  url.hash = `#/share/${token}`
  return url.toString()
}

export function PlaygroundShareControl({ sessions, streaming }: PlaygroundShareControlProps) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const [ttlDays, setTtlDays] = useState<1 | 7 | 30>(7)
  const [issued, setIssued] = useState<PlaygroundShareCreateResponse>()
  const [revoked, setRevoked] = useState(false)
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const snapshot = useMemo(() => buildPlaygroundShareSnapshot(sessions), [sessions])
  const createMutation = useCreatePlaygroundShare()
  const revokeMutation = useRevokePlaygroundShare()
  const expired = useShareExpired(issued?.expires_at)
  const hasConversation = sessions.some((session) => session.messages.length > 0)
  const canOpen = issued !== undefined || (hasConversation && !streaming)

  const create = async () => {
    if (!snapshot.ok || streaming) return
    try {
      const result = await createMutation.mutateAsync({ sessions: snapshot.sessions, ttlDays })
      setIssued(result)
      setRevoked(false)
      setCopyState('idle')
    } catch {
      // Mutation 状态负责呈现稳定错误，响应正文不会进入界面。
    }
  }

  const copy = async () => {
    if (!issued) return
    setCopyState(await copyText(currentShareUrl(issued.token)) ? 'copied' : 'failed')
  }

  const revoke = async () => {
    if (!issued) return
    try {
      await revokeMutation.mutateAsync(issued.token)
      setRevoked(true)
    } catch {
      // 保留现有链接，允许用户重试或关闭对话框。
    }
  }

  const reset = () => {
    setIssued(undefined)
    setRevoked(false)
    setCopyState('idle')
    createMutation.reset()
    revokeMutation.reset()
  }

  const createError = createMutation.isError
    ? classifyShareMutationError(createMutation.error)
    : undefined

  return (
    <>
      <Button
        type="button"
        size="sm"
        variant="secondary"
        className="px-2.5"
        disabled={!canOpen}
        title={t(streaming ? 'playground.share.waitForCompletion' : 'playground.share.action')}
        aria-label={t('playground.share.action')}
        onClick={() => setOpen(true)}
      >
        {issued && !revoked && !expired ? <Check aria-hidden="true" /> : <Share2 aria-hidden="true" />}
        <span className="hidden md:inline">{t('playground.share.action')}</span>
      </Button>

      <AlertDialog open={open} onOpenChange={setOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <div className="grid size-10 place-items-center rounded-lg bg-info/10 text-info">
              <ShieldCheck className="size-4" aria-hidden="true" />
            </div>
            <AlertDialogTitle>{t('playground.share.title')}</AlertDialogTitle>
            <AlertDialogDescription>{t('playground.share.description')}</AlertDialogDescription>
          </AlertDialogHeader>

          {issued ? (
            <PlaygroundShareIssued
              copied={copyState === 'copied'}
              copyFailed={copyState === 'failed'}
              expired={expired}
              issued={issued}
              revoked={revoked}
              revoking={revokeMutation.isPending}
              revokeFailed={revokeMutation.isError}
              url={currentShareUrl(issued.token)}
              onCopy={() => void copy()}
              onReset={reset}
              onRevoke={() => void revoke()}
            />
          ) : (
            <div className="grid gap-4">
              <div className="grid gap-2">
                <label htmlFor="playground-share-ttl" className="text-sm font-medium">
                  {t('playground.share.ttlLabel')}
                </label>
                <Select
                  id="playground-share-ttl"
                  value={ttlDays}
                  onChange={(event) => setTtlDays(Number(event.target.value) as 1 | 7 | 30)}
                >
                  {[1, 7, 30].map((days) => (
                    <option key={days} value={days}>{t('playground.share.ttlDays', { count: days })}</option>
                  ))}
                </Select>
                <p className="text-xs leading-5 text-muted-foreground">{t('playground.share.scope')}</p>
              </div>

              {!snapshot.ok ? (
                <p role="alert" className="rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
                  {t(`playground.share.snapshotErrors.${snapshot.reason}`)}
                </p>
              ) : streaming ? (
                <p role="status" className="rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
                  {t('playground.share.waitForCompletion')}
                </p>
              ) : null}
              {createError ? (
                <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                  {t(`playground.share.createErrors.${createError}`)}
                </p>
              ) : null}
            </div>
          )}

          <AlertDialogFooter>
            <AlertDialogCancel>{t(issued ? 'playground.share.done' : 'playground.share.cancel')}</AlertDialogCancel>
            {!issued ? (
              <Button type="button" disabled={!snapshot.ok || streaming || createMutation.isPending} onClick={() => void create()}>
                {createMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Share2 aria-hidden="true" />}
                {t(createMutation.isPending ? 'playground.share.creating' : 'playground.share.create')}
              </Button>
            ) : null}
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
}
