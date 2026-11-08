import { Button } from '@heroui/react'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { buttonSecondaryLg } from './auth-form-styles'
import type { OAuthLoginProvider } from './oauth-login-api'
import { OAuthProviderIcon } from './oauth-provider-icon'
import { cn } from '@/lib/utils'

type OAuthLoginButtonsProps = {
  providers: ReadonlyArray<{ id: string; display_name: string }>
  pendingProvider?: OAuthLoginProvider
  disabled?: boolean
  onSelect: (provider: OAuthLoginProvider) => void
}

/** Provider controls shared by login and registration; callers own invitation validation. */
export function OAuthLoginButtons({ providers, pendingProvider, disabled, onSelect }: OAuthLoginButtonsProps) {
  const { t } = useTranslation()
  return providers.map((provider) => {
    const providerId = provider.id as OAuthLoginProvider
    const isStarting = pendingProvider === providerId
    const custom = provider.id.startsWith('custom_')
    const label = custom
      ? t(isStarting ? 'auth.login.oauthStarting.custom' : 'auth.login.oauth.custom', { provider: provider.display_name })
      : t(`auth.login.${isStarting ? 'oauthStarting' : 'oauth'}.${provider.id}`)
    return (
      <Button
        key={provider.id}
        data-slot="button"
        type="button"
        size="lg"
        variant="flat"
        className={cn(buttonSecondaryLg, 'h-auto min-h-11 w-full whitespace-normal break-words py-3 text-center leading-5')}
        isDisabled={disabled || Boolean(pendingProvider)}
        onPress={() => onSelect(providerId)}
      >
        {isStarting ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <OAuthProviderIcon provider={providerId} />}
        {label}
      </Button>
    )
  })
}
