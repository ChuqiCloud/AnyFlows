# Service identity contracts

`GatewayIdentity` distinguishes an authenticated user principal from an independent
service identity. User principals retain their validated user, token and group IDs.
Service identities contain an organization, service account and credential identifier;
they do not provide substitute user or member IDs.

`af-admin` exposes `ServiceAccountAuthenticator` for distribution-owned credential
storage. The caller parses a `PresentedServiceAccountKey`, computes its digest,
and drops the plaintext before awaiting authentication. The key format and
domain-separated SHA-256 digest remain compatible with previously issued credentials.
The plaintext and digest types clear their owned strings on drop and redact `Debug`.

The adapter returns `ServiceAccountAuthentication` with validated identity, optional
team and department scope. It must check the current credential and account lifecycle
and organizational authorization on every request. Storage failure is distinct from
an invalid credential; public responses must not disclose the reason for rejection.

This port validates identity only. It does not create a login session, select a
routing group, authorize a model, reserve funds or enable the existing user gateway
pipeline for service keys. Routing, admission, billing, logging and asynchronous task
ownership must handle the service variant explicitly before model execution is enabled.
Database entities, authorization rules and private routes remain in the extension.
