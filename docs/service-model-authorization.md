# Service model authorization

`af-admin::ServiceAccountModelAuthenticator` resolves an independent service
credential and its current model authorization in one storage snapshot. It
accepts a `ServiceAccountKeyDigest`; it does not accept a user ID, personal Token
or creator identity.

The returned `ServiceAccountModelAuthentication` contains the service identity,
an explicitly granted routing group, a `ServiceModelPolicy`, and positive RPM
and concurrency limits. The limits are authorization facts; consumers must
enforce them with service account namespaces before executing requests.

`ServiceModelPolicy` uses exact, case-sensitive canonical model names. An empty
allowlist denies all models. Intersecting subordinate policies can only narrow
the original grant. Policy construction bounds entry count, individual name
length and total text size before removing duplicate names.

Registering this port does not enable model forwarding. A host must separately
connect service routing, rate and concurrency enforcement, billing, request
outcomes and asynchronous task ownership. Service identities must retain their
original account and credential throughout settlement; they must not be adapted
into a `GatewayPrincipal`.
