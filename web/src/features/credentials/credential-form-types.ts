import type {
  AdminCredentialCreateRequestWritable,
  AdminCredentialMultiKeyMode,
  AdminCredentialQuotaDimension,
  AdminRoutingWriteStatus,
} from '@/lib/api/generated/types.gen'

export type CredentialFormKind = AdminCredentialCreateRequestWritable['kind']

export type CredentialFormValues = {
  kind: CredentialFormKind
  rotateSecret: boolean
  apiKey: string
  accessToken: string
  accessKeyId: string
  secretAccessKey: string
  sessionToken: string
  clientEmail: string
  privateKeyId: string
  privateKey: string
  status: AdminRoutingWriteStatus | 'auto_disabled'
  schedulable: boolean
  multiKeyMode: AdminCredentialMultiKeyMode | 'none'
  priority: string
  weight: string
  concurrency: string
  loadFactor: string
  rateMultiplier: string
  parentId: string
  proxyId: string
  quotaDimension: AdminCredentialQuotaDimension
  oauthProvider: string
  oauthAccountKey: string
  oauthProjectId: string
}

export type CredentialValidationMessages = {
  incompatibleKind: string
  secret: string
  oauthProvider: string
  serviceAccountEmail: string
  privateKey: string
  status: string
  integer: string
  nonNegativeInteger: string
  multiplier: string
  parent: string
  optionalText: string
}
