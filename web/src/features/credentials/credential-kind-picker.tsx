import { CloudCog, FileKey2, KeyRound, Link2 } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { WritableCredentialKind } from './credential-model'

const kindIcons: Record<WritableCredentialKind, LucideIcon> = {
  api_key: KeyRound,
  oauth: Link2,
  bedrock: CloudCog,
  service_account: FileKey2,
}

export function CredentialKindPicker({ value, kinds, disabled, onChange }: {
  value: WritableCredentialKind
  kinds: readonly WritableCredentialKind[]
  disabled?: boolean
  onChange: (kind: WritableCredentialKind) => void
}) {
  const { t } = useTranslation()
  return (
    <div
      className={cn('grid gap-1 rounded-lg bg-surface-2 p-1', kinds.length === 1 ? 'grid-cols-1' : 'grid-cols-2')}
      role="radiogroup"
      aria-label={t('credentials.form.kind')}
    >
      {kinds.map((kind) => {
        const Icon = kindIcons[kind]
        const active = kind === value
        return (
          <Button
            key={kind}
            type="button"
            size="sm"
            variant="ghost"
            role="radio"
            aria-checked={active}
            disabled={disabled}
            className={cn(
              'h-12 w-full flex-col gap-1 text-[0.6875rem]',
              active && 'bg-background text-foreground shadow-xs hover:bg-background',
            )}
            onClick={() => onChange(kind)}
          >
            <Icon className="size-4" aria-hidden="true" />
            {t(`credentials.kind.${kind}`)}
          </Button>
        )
      })}
    </div>
  )
}

export { kindIcons as credentialKindIcons }
