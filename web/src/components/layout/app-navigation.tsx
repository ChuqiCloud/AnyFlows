import { Gauge, KeyRound, Network, ReceiptText, Settings, WalletCards } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type AppNavigationProps = {
  className?: string
  compact?: boolean
}

const navItems = [
  { key: 'overview', icon: Gauge },
  { key: 'channels', icon: Network },
  { key: 'keys', icon: KeyRound },
  { key: 'usage', icon: ReceiptText },
  { key: 'billing', icon: WalletCards },
  { key: 'settings', icon: Settings },
]

export function AppNavigation({ className, compact = false }: AppNavigationProps) {
  const { t } = useTranslation()

  return (
    <nav
      className={cn(
        'flex gap-1 overflow-x-auto',
        compact && '[scrollbar-width:none] [&::-webkit-scrollbar]:hidden',
        className,
      )}
      aria-label={t('nav.label')}
    >
      {navItems.map((item, index) => {
        const IconComponent = item.icon
        const active = index === 0

        return (
          <Button
            key={item.key}
            type="button"
            size={compact ? 'sm' : 'default'}
            variant={active ? 'secondary' : 'ghost'}
            className={cn(
              'shrink-0 justify-start gap-2.5',
              compact ? 'h-8 px-3' : 'h-10 px-3',
              active && 'bg-sidebar-accent',
            )}
            aria-current={active ? 'page' : undefined}
          >
            <IconComponent className="size-4" aria-hidden="true" />
            {t(`nav.${item.key}`)}
          </Button>
        )
      })}
    </nav>
  )
}
