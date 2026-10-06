import { Check, Copy, Download, TicketCheck, TriangleAlert } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
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
import { Textarea } from '@/components/ui/textarea'
import type { IssuedAdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { copyText } from '@/lib/clipboard'

type RedemptionIssuedDialogProps = {
  issued?: IssuedAdminRedemptionBatch
  onClose: () => void
}

export function RedemptionIssuedDialog({ issued, onClose }: RedemptionIssuedDialogProps) {
  const { t } = useTranslation()
  const [copyState, setCopyState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const codeText = useMemo(() => issued?.codes.join('\n') ?? '', [issued])

  useEffect(() => setCopyState('idle'), [issued?.batch.batch_id])

  const copy = async () => {
    if (!codeText) return
    setCopyState(await copyText(codeText) ? 'copied' : 'failed')
  }

  const download = () => {
    if (!issued || !codeText) return
    const url = URL.createObjectURL(new Blob([`${codeText}\n`], { type: 'text/plain;charset=utf-8' }))
    const link = document.createElement('a')
    link.href = url
    link.download = `redemption-codes-${issued.batch.batch_id}.txt`
    link.click()
    URL.revokeObjectURL(url)
  }

  return (
    <AlertDialog open={issued !== undefined} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent className="max-h-[calc(100vh-2rem)] max-w-2xl overflow-y-auto">
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success">
            <TicketCheck className="size-4" aria-hidden="true" />
          </div>
          <AlertDialogTitle>{t('redemptions.issued.title')}</AlertDialogTitle>
          <AlertDialogDescription>
            {t('redemptions.issued.description', {
              count: issued?.codes.length ?? 0,
              name: issued?.batch.name,
            })}
          </AlertDialogDescription>
        </AlertDialogHeader>

        <Textarea
          className="min-h-56 resize-y font-mono text-xs leading-5"
          value={codeText}
          readOnly
          spellCheck={false}
          onFocus={(event) => event.currentTarget.select()}
        />
        {copyState === 'failed' ? (
          <p role="alert" className="text-xs text-destructive">{t('redemptions.issued.copyFailed')}</p>
        ) : null}
        <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
          {t('redemptions.issued.warning')}
        </p>

        <AlertDialogFooter>
          <Button type="button" variant="secondary" onClick={download}>
            <Download aria-hidden="true" />
            {t('redemptions.issued.download')}
          </Button>
          <Button type="button" variant="secondary" onClick={() => void copy()}>
            {copyState === 'copied' ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
            {t(copyState === 'copied' ? 'redemptions.issued.copied' : 'redemptions.issued.copy')}
          </Button>
          <AlertDialogAction onClick={onClose}>{t('redemptions.issued.done')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
