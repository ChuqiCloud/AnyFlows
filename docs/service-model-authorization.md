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

Hosts register this port through `RuntimeExtensions::service_account_models`,
together with service billing and usage storage. The runtime enforces RPM and
concurrency in Redis using the organization and service account as a composite
subject. Rotating a credential does not reset either limit. Missing admission
storage rejects service requests.

Service credentials can use `GET /v1/models` and the synchronous Chat Completions,
Responses, Anthropic Messages and Gemini generation endpoints, including streams.
Only one Bearer header is accepted; query keys and vendor key headers are rejected.
The directory and request policy use the explicit routing group and canonical
model allowlist. Reasoning suffixes are normalized before the allowlist check.
Routing does not borrow creator groups or playground fallback rules.

Precharge retries retain the reservation, original identity, amount and absolute
expiry. Settlement and cancellation bind the original credential even after
rotation. Usage is projected to distribution storage without personal rows.
Sticky sessions include organization, account and credential identifiers in a
separate hash domain. Personal session hashes retain their existing format.

Audio, embeddings, images, rerank, compact and asynchronous video endpoints remain
restricted to personal tokens. Service failure history and persistent recovery
of unfinished billing require separate distribution contracts. Synchronous
service requests do not write failures under a creator or member identity.
