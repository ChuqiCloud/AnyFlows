import { Check, Copy, Link2Off, RotateCcw, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type { PlaygroundShareCreateResponse } from '@/lib/api/generated/types.gen'

type PlaygroundShareIssuedProps = {
  copied: boolean
  copyFailed: boolean
  expired: boolean
  issued: PlaygroundShareCreateResponse
  revoked: boolean
  revoking: boolean
  revokeFailed: boolean
  url: string
  onCopy: () => void
  onReset: () => void
  onRevoke: () => void
}

export function PlaygroundShareIssued(props: PlaygroundShareIssuedProps) {
  const { i18n, t } = useTranslation()
  const [confirmingRevoke, setConfirmingRevoke] = useState(false)
  const inactive = props.expired || props.revoked
  const status = props.revoked ? 'revoked' : props.expired ? 'expired' : 'active'
  const expiresAt = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(props.issued.expires_at * 1000)

  return (
    <div className="grid gap-4">
      <div className="flex items-center justify-between gap-3 rounded-lg bg-surface-2/55 px-3 py-2.5">
        <div className="min-w-0">
          <p className="text-xs font-medium">{t('playground.share.issuedLabel')}</p>
          <p className="mt-0.5 truncate text-[0.6875rem] text-muted-foreground">
            {t('playground.share.expiresAt', { value: expiresAt })}
          </p>
        </div>
        <Badge className={inactive ? 'bg-warning/12 text-warning' : 'bg-success/12 text-success'}>
          {t(`playground.share.status.${status}`)}
        </Badge>
      </div>

      <div className="grid gap-2">
        <label htmlFor="playground-share-url" className="text-sm font-medium">
          {t('playground.share.linkLabel')}
        </label>
        <div className="flex gap-2">
          <Input
            id="playground-share-url"
            className="min-w-0 font-mono text-xs"
            value={props.url}
            readOnly
            onFocus={(event) => event.currentTarget.select()}
          />
          <Button type="button" variant="secondary" className="shrink-0" onClick={props.onCopy}>
            {props.copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
            {t(props.copied ? 'playground.share.copied' : 'playground.share.copy')}
          </Button>
        </div>
        {props.copyFailed ? (
          <p role="alert" className="text-xs text-destructive">{t('playground.share.copyFailed')}</p>
        ) : null}
      </div>

      {inactive ? (
        <Button type="button" variant="secondary" onClick={props.onReset}>
          <RotateCcw aria-hidden="true" />{t('playground.share.createAgain')}
        </Button>
      ) : confirmingRevoke ? (
        <div className="grid gap-2 rounded-lg bg-destructive/8 px-3 py-2.5">
          <p className="flex items-start gap-2 text-xs leading-5 text-destructive">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            {t('playground.share.revokeConfirm')}
          </p>
          <div className="flex justify-end gap-2">
            <Button type="button" size="sm" variant="ghost" onClick={() => setConfirmingRevoke(false)}>
              {t('playground.share.keep')}
            </Button>
            <Button type="button" size="sm" variant="destructive" disabled={props.revoking} onClick={props.onRevoke}>
              <Link2Off aria-hidden="true" />
              {t(props.revoking ? 'playground.share.revoking' : 'playground.share.revokeConfirmAction')}
            </Button>
          </div>
        </div>
      ) : (
        <Button type="button" variant="destructive" onClick={() => setConfirmingRevoke(true)}>
          <Link2Off aria-hidden="true" />{t('playground.share.revoke')}
        </Button>
      )}

      {props.revokeFailed ? (
        <p role="alert" className="text-xs text-destructive">{t('playground.share.revokeFailed')}</p>
      ) : null}
    </div>
  )
}
