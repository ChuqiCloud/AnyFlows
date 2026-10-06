import type { AdminOAuthAuthorizationResponse } from '@/lib/api/generated/types.gen'
import type { OAuthCredentialBaseline } from './credential-oauth-model'

export type CredentialOAuthStatus = 'idle' | 'waiting' | 'interrupted' | 'connected' | 'error' | 'expired'

export type ActiveOAuthAuthorization = AdminOAuthAuthorizationResponse & {
  baseline: OAuthCredentialBaseline
  expiresAt: number
}
