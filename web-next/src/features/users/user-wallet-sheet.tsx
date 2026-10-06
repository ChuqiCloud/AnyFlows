import { Chip, Drawer, DrawerBody, DrawerContent, DrawerHeader } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
    <Drawer
      aria-describedby="user-wallet-description"
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
              <div className="flex min-w-0 items-center gap-2">
                <h2 className="truncate text-base font-medium text-foreground">{t('users.wallet.title')}</h2>
                {user ? <Chip className="shrink-0" size="sm" variant="flat">{user.username}</Chip> : null}
              </div>
              <p className="text-sm text-muted-foreground" id="user-wallet-description">{t('users.wallet.description')}</p>
            </DrawerHeader>
            {user ? (
              <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
                <div className="grid shrink-0 grid-cols-3 divide-x divide-[var(--hairline)] border-b border-[var(--hairline)] bg-surface-2/35">
                  <WalletMetric label={t('users.wallet.metrics.available')} value={formatQuota(user.quota)} />
                  <WalletMetric label={t('users.wallet.metrics.used')} value={formatQuota(user.used_quota)} />
                  <WalletMetric label={t('users.wallet.metrics.frozen')} value={formatQuota(user.frozen_quota)} />
                </div>
                <UserWalletForm key={user.id} user={user} />
                <UserWalletHistory open={open} userId={user.id} />
              </DrawerBody>
            ) : null}
          </>
        )}
      </DrawerContent>
    </Drawer>
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
