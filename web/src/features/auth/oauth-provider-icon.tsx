import { Globe2, MessageCircle, QrCode, Send, ShieldCheck, type LucideIcon } from 'lucide-react'
import GithubIcon from '@lobehub/icons/es/Github/components/Mono'
import GoogleIcon from '@lobehub/icons/es/Google/components/Color'

import { cn } from '@/lib/utils'

import type { OAuthLoginProvider } from './oauth-login-api'

type OAuthProviderIconProps = {
  provider: OAuthLoginProvider
  className?: string
}

const lucideIcons: Partial<Record<OAuthLoginProvider, LucideIcon>> = {
  discord: MessageCircle,
  oidc: ShieldCheck,
  linuxdo: Globe2,
  wechat: QrCode,
  telegram: Send,
}

/** 统一登录入口与管理员设置中的 Provider 图标，保证尺寸和无障碍语义一致。 */
export function OAuthProviderIcon({ provider, className }: OAuthProviderIconProps) {
  const iconClassName = cn('size-4 shrink-0', className)
  if (provider === 'github' || provider === 'google') {
    return (
      <span className={cn('inline-flex items-center justify-center', iconClassName)} aria-hidden="true">
        {provider === 'github' ? <GithubIcon size="1rem" /> : <GoogleIcon size="1rem" />}
      </span>
    )
  }
  const Icon = lucideIcons[provider] ?? ShieldCheck
  return <Icon className={iconClassName} aria-hidden="true" />
}
