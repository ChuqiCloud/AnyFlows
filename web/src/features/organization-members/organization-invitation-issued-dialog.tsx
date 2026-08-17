import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AlertDialog, AlertDialogAction, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

/** 邀请令牌只展示一次；对话框关闭后立即由父组件释放明文。 */
export function OrganizationInvitationIssuedDialog({
  organizationName,
  token,
  onClose,
}: {
  organizationName: string
  token?: string
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return undefined
    const timer = window.setTimeout(() => setCopied(false), 2_000)
    return () => window.clearTimeout(timer)
  }, [copied])
  const copy = async () => {
    if (!token) return
    try {
      await navigator.clipboard.writeText(token)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }
  return (
    <AlertDialog open={token !== undefined} onOpenChange={(open) => { if (!open) onClose() }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{t('organizationConsole.invitations.issued.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('organizationConsole.invitations.issued.body', { organization: organizationName })}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex gap-2">
          <Input value={token ?? ''} readOnly className="min-w-0 font-mono text-xs" />
          <Button type="button" size="icon" variant="outline" aria-label={t('organizationConsole.actions.copy')} onClick={() => void copy()}>
            {copied ? <Check className="text-success" aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </Button>
        </div>
        <p className="text-xs leading-5 text-muted-foreground">{t('organizationConsole.invitations.issued.hint')}</p>
        <AlertDialogFooter>
          <AlertDialogAction onClick={onClose}>{t('organizationConsole.actions.done')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
