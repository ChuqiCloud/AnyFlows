import { Check, Copy, KeyRound } from 'lucide-react'
import { useEffect, useState } from 'react'
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
import { Input } from '@/components/ui/input'
import type { OrganizationServiceAccountIssuedResponse } from '@/lib/api/generated/types.gen'

export function OrganizationServiceAccountKeyDialog({
  issued,
  onClose,
}: {
  issued?: OrganizationServiceAccountIssuedResponse
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    setCopied(false)
  }, [issued])

  const copy = async () => {
    if (!issued) return
    try {
      await navigator.clipboard.writeText(issued.secret)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <AlertDialog open={issued !== undefined} onOpenChange={(open) => !open && onClose()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <div className="grid size-10 place-items-center rounded-lg bg-success/10 text-success">
            <KeyRound className="size-4" aria-hidden="true" />
          </div>
          <AlertDialogTitle>{t('organizationServiceAccounts.keyDialog.title')}</AlertDialogTitle>
          <AlertDialogDescription>{t('organizationServiceAccounts.keyDialog.description')}</AlertDialogDescription>
        </AlertDialogHeader>
        {issued ? (
          <div className="grid gap-3">
            <div className="grid gap-1.5">
              <label htmlFor="organization-service-account-secret" className="text-xs font-medium">
                {t('organizationServiceAccounts.keyDialog.secretLabel')}
              </label>
              <div className="flex gap-2">
                <Input id="organization-service-account-secret" readOnly value={issued.secret} className="font-mono text-xs" />
                <button type="button" className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-lg border border-[var(--hairline)] px-2.5 text-xs font-medium hover:bg-surface-2 focus-visible:ring-2 focus-visible:ring-ring/60" onClick={() => void copy()}>
                  {copied ? <Check className="size-3.5 text-success" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
                  {t(copied ? 'organizationServiceAccounts.keyDialog.copied' : 'organizationServiceAccounts.keyDialog.copy')}
                </button>
              </div>
            </div>
            <dl className="grid grid-cols-2 gap-3 rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-3 text-xs">
              <div>
                <dt className="text-muted-foreground">{t('organizationServiceAccounts.keyDialog.prefix')}</dt>
                <dd className="mt-1 font-mono font-medium">{issued.key_prefix}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground">{t('organizationServiceAccounts.keyDialog.keyId')}</dt>
                <dd className="mt-1 font-mono font-medium">{issued.key_key}</dd>
              </div>
            </dl>
            <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">
              {t('organizationServiceAccounts.keyDialog.warning')}
            </p>
          </div>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogAction onClick={onClose}>{t('organizationServiceAccounts.actions.done')}</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
