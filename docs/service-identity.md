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

Billing usage records have one `GatewayIdentity`. Service records expose no user or
token principal. A `BillingRequestPlan` has separate service activation methods;
metered activation binds the original service identity to `ServiceAccountBillingPort`
for settlement and non-blocking cancellation compensation. Callers must first confirm
precharge and reuse its absolute expiry on retries.

`DatabaseUsageRecordSink` sends service facts directly to the distribution's
`UsageRecordProjection`. Missing service storage returns an error and keeps the
delivery pending. Personal usage continues through `UsageLogRepository`. Service
facts never create personal usage rows.

The billing port does not authorize model execution. Hosts also register the
model authorization port for synchronous model endpoints; see
[Service model authorization](service-model-authorization.md). Service identities
are never converted to personal principals. Asynchronous task ownership remains
outside this registration.

Synchronous service requests send terminal facts to `ServiceAccountOutcomeSink`,
registered through `RuntimeExtensions::service_account_outcomes`. Successful,
failed and interrupted streams retain the identity fixed at admission. These facts
never enter personal request logs. They contain request and model identifiers,
protocol, operation, routing group, optional channel, duration and a closed failure
category. Request bodies, upstream response text and credential secrets are excluded.

The sink must append idempotently and reject conflicting replays. Transient storage
errors receive at most three attempts using the same fact. Logging failures leave the
model response and settlement unchanged. Stream cancellation hands off one asynchronous
write; this is best-effort delivery and does not provide recovery after process exit.
The distribution owns storage and any subsequent authorized read API.
