import { z } from 'zod'

import type { AdminChannelType } from '@/lib/api/generated/types.gen'
import { credentialKindsForChannel } from './credential-model'
import {
  parseMicros,
  validAsciiComponent,
  validInteger,
  validOAuthToken,
  validOptionalText,
  validPrivateKeyPem,
  validServiceAccountEmail,
  validSimpleSecret,
} from './credential-form-scalar'
import type { CredentialValidationMessages } from './credential-form-types'

type CredentialSchemaOptions = {
  channelType: AdminChannelType
  channelProvider?: string | null
  allowSparkShadow: boolean
  creating: boolean
  currentId?: number
  messages: CredentialValidationMessages
}

const credentialFormShape = z.object({
  mode: z.enum(['standard', 'spark_shadow']),
  kind: z.enum(['api_key', 'oauth', 'bedrock', 'service_account']),
  oauthCreateMode: z.enum(['authorize', 'access_token']),
  rotateSecret: z.boolean(),
  apiKey: z.string(), accessToken: z.string(), accessKeyId: z.string(),
  secretAccessKey: z.string(), sessionToken: z.string(), clientEmail: z.string(),
  privateKeyId: z.string(), privateKey: z.string(),
  status: z.enum(['enabled', 'disabled', 'auto_disabled']),
  schedulable: z.boolean(),
  multiKeyMode: z.enum(['none', 'random', 'round_robin']),
  priority: z.string(), weight: z.string(), concurrency: z.string(),
  loadFactor: z.string(), rateMultiplier: z.string(), parentId: z.string(), proxyId: z.string(),
  quotaDimension: z.enum(['global', 'spark']),
  oauthProvider: z.string(), oauthAccountKey: z.string(), oauthProjectId: z.string(),
})

/** 构造与服务端闭合契约一致的动态校验。 */
export function buildCredentialFormSchema(options: CredentialSchemaOptions) {
  return credentialFormShape.superRefine((values, context) => {
    const message = options.messages
    if (values.mode === 'spark_shadow') {
      if (!options.allowSparkShadow
        || values.kind !== 'oauth'
        || values.quotaDimension !== 'spark'
        || values.parentId === ''
        || values.concurrency !== ''
        || values.proxyId !== '') {
        issue(context, 'parentId', message.parent)
      }
      if (values.parentId !== '' && (!validInteger(values.parentId, 1, Number.MAX_SAFE_INTEGER)
        || Number(values.parentId) === options.currentId)) issue(context, 'parentId', message.parent)
      validateScheduling(values, context, message)
      return
    }
    if (options.creating && !credentialKindsForChannel(options.channelType, options.channelProvider).includes(values.kind)) {
      issue(context, 'kind', message.incompatibleKind)
    }
    validateScheduling(values, context, message)
    if (values.parentId !== '' || values.quotaDimension !== 'global') issue(context, 'parentId', message.parent)
    if (values.proxyId !== '' && !validInteger(values.proxyId, 1, Number.MAX_SAFE_INTEGER)) {
      issue(context, 'proxyId', message.parent)
    }
    for (const field of ['oauthAccountKey', 'oauthProjectId'] as const) {
      if (!validOptionalText(values[field], 255)) issue(context, field, message.optionalText)
    }
    if (values.kind === 'oauth'
      && (!validOptionalText(values.oauthProvider, 64) || values.oauthProvider === '')) {
      issue(context, 'oauthProvider', message.oauthProvider)
    }

    if (options.creating && values.kind === 'oauth' && values.oauthCreateMode === 'authorize') return
    if (!options.creating && !values.rotateSecret) return
    validateSecret(values, context, message)
  })
}

function validateScheduling(
  values: z.infer<typeof credentialFormShape>,
  context: z.RefinementCtx,
  message: CredentialValidationMessages,
) {
  if (values.status === 'auto_disabled') issue(context, 'status', message.status)
  if (!validInteger(values.priority, -2_147_483_648, 2_147_483_647)) issue(context, 'priority', message.integer)
  if (!validInteger(values.weight, 0, 2_147_483_647)) issue(context, 'weight', message.nonNegativeInteger)
  if (values.concurrency !== '' && !validInteger(values.concurrency, 0, 2_147_483_647)) {
    issue(context, 'concurrency', message.nonNegativeInteger)
  }
  for (const field of ['loadFactor', 'rateMultiplier'] as const) {
    if (values[field] !== '' && parseMicros(values[field]) === undefined) issue(context, field, message.multiplier)
  }
}

function validateSecret(
  values: z.infer<typeof credentialFormShape>,
  context: z.RefinementCtx,
  message: CredentialValidationMessages,
) {
  switch (values.kind) {
    case 'api_key':
      if (!validSimpleSecret(values.apiKey)) issue(context, 'apiKey', message.secret)
      break
    case 'oauth':
      if (!validOAuthToken(values.accessToken)) issue(context, 'accessToken', message.secret)
      break
    case 'bedrock':
      if (!validAsciiComponent(values.accessKeyId, 128)) issue(context, 'accessKeyId', message.secret)
      if (!validAsciiComponent(values.secretAccessKey, 4 * 1_024)) issue(context, 'secretAccessKey', message.secret)
      if (values.sessionToken !== '' && !validAsciiComponent(values.sessionToken, 16 * 1_024)) issue(context, 'sessionToken', message.secret)
      break
    case 'service_account':
      if (!validServiceAccountEmail(values.clientEmail)) issue(context, 'clientEmail', message.serviceAccountEmail)
      if (values.privateKeyId !== '' && !validAsciiComponent(values.privateKeyId, 128)) issue(context, 'privateKeyId', message.secret)
      if (!validPrivateKeyPem(values.privateKey)) issue(context, 'privateKey', message.privateKey)
      break
  }
}

function issue(context: z.RefinementCtx, path: string, message: string) {
  context.addIssue({ code: 'custom', path: [path], message })
}
