import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { UserWalletForm } from './user-wallet-form'
import { UserWalletHistory } from './user-wallet-history'

type UserWalletSheetProps = {
  open: boolean
  user?: AdminUser
  onOpenChange: (open: boolean) => void
}

export function UserWalletSheet({ open, user, onOpenChange }: UserWalletSheetProps) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="user-wallet-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <div className="flex min-w-0 items-center gap-2">
            <SheetTitle className="truncate">{t('users.wallet.title')}</SheetTitle>
            {user ? <Badge className="shrink-0">{user.username}</Badge> : null}
          </div>
          <SheetDescription id="user-wallet-description">{t('users.wallet.description')}</SheetDescription>
        </SheetHeader>
        {user ? (
          <>
            <div className="grid grid-cols-3 divide-x divide-[var(--hairline)] border-b border-[var(--hairline)] bg-surface-2/35">
              <WalletMetric label={t('users.wallet.metrics.available')} value={formatQuota(user.quota)} />
              <WalletMetric label={t('users.wallet.metrics.used')} value={formatQuota(user.used_quota)} />
              <WalletMetric label={t('users.wallet.metrics.frozen')} value={formatQuota(user.frozen_quota)} />
            </div>
            <UserWalletForm key={user.id} user={user} />
            <UserWalletHistory open={open} userId={user.id} />
          </>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}

function WalletMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 px-3 py-3 text-center">
      <div className="truncate text-[0.625rem] text-muted-foreground">{label}</div>
      <div className="mt-1 truncate text-xs font-semibold tabular-nums" title={value}>{value}</div>
    </div>
  )
}
