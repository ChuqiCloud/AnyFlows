import { useEffect, useState } from 'react'
import { Check, Copy, KeyRound, MessageSquareText, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type { IssuedUserToken } from '@/lib/api/generated/types.gen'
import { copyText } from '@/lib/clipboard'

type IssuedApiKeyDialogProps = {
  issued?: IssuedUserToken
  onClose: () => void
  showPlayground?: boolean
}

export function IssuedApiKeyDialog({ issued, onClose, showPlayground = true }: IssuedApiKeyDialogProps) {
  const { t } = useTranslation()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')

  useEffect(() => setCopyState('idle'), [issued?.token.id])

  const copy = async () => {
    if (!issued) return
    setCopyState(await copyText(issued.api_key) ? 'copied' : 'failed')
  }

  const openPlayground = () => {
    if (!issued) return
    onClose()
    window.location.hash = '#/console/playground'
  }

  return (
    <AlertDialog open={issued !== undefined} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success"><KeyRound className="size-4" aria-hidden="true" /></div>
          <AlertDialogTitle>{t('apiKeys.issued.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('apiKeys.issued.description', { name: issued?.token.name })}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="grid gap-2">
          <div className="flex gap-2">
            <Input className="min-w-0 font-mono text-xs" value={issued?.api_key ?? ''} readOnly onFocus={(event) => event.currentTarget.select()} />
            <Button type="button" variant="secondary" className="shrink-0" onClick={() => void copy()}>
              {copyState === 'copied' ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
              {t(copyState === 'copied' ? 'apiKeys.issued.copied' : 'apiKeys.issued.copy')}
            </Button>
          </div>
          {copyState === 'failed' ? <p role="alert" className="text-xs text-destructive">{t('apiKeys.issued.copyFailed')}</p> : null}
          <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
            <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            {t('apiKeys.issued.warning')}
          </p>
        </div>
        <AlertDialogFooter>
          {showPlayground ? (
            <Button type="button" variant="secondary" onClick={openPlayground}>
              <MessageSquareText aria-hidden="true" />{t('apiKeys.issued.playground')}
            </Button>
          ) : null}
          <AlertDialogAction onClick={onClose}>{t('apiKeys.issued.done')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
