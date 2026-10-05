# Repository Boundary

This document defines the first extraction boundary for the public core, the private enterprise extensions, and the private distribution repository.

## Public core

The public repository owns model gateway protocols, provider routing, credentials and channels, usage accounting, request logs, shared authentication, common administration, and the two shared console shells.

## Enterprise extensions

The private enterprise repository owns organizations, memberships, departments, enterprise wallets, approvals, SSO, SCIM, enterprise verification, enterprise plans, enterprise service accounts, and the corresponding API and console pages.

## Distribution

The private distribution repository owns the selected public core revision, the compatible enterprise revision, release metadata, deployment overlays, and packaging configuration. It must not contain an independent copy of either codebase.

## Extraction rules

1. New enterprise database migrations remain outside the public migration chain.
   The public `af-db` crate exposes `run_pending_migrations_with` and
   `connect_and_migrate_with` for an extension-owned `MigratorTrait`; the
   extension must override `migration_table_name` and use its own migration
   table. `MigratorExtension` rejects the historical public table at runtime so
   a distribution cannot silently write enterprise history into
   `seaql_migrations`. Existing installations still contain earlier enterprise
   records in that historical table and require an explicit adoption step before
   the remaining enterprise migrations are moved out of the in-tree registry.
   The adoption step uses `MigrationHistoryAdoption` with the exact migration
   names owned by the extension. `MigratorExtension::with_legacy_history` copies
   only matching rows, preserves `applied_at`, and then runs pending extension
   migrations. It does nothing when the old table is absent, so the same startup
   path works for new installations. Versions not present in the extension
   registry are rejected rather than copied.
2. Enterprise routes and OpenAPI documents are registered through an extension boundary.
   The built-in route composition accepts `af_http::OrganizationHttpServices` as one
   dependency bundle, and `af-http` assembles those services in a dedicated organization
   route module. A distribution can assemble enterprise services in its own startup
   module without expanding the public router function signature or placing enterprise
   route wiring in the shared router composition.
3. Frontend navigation is driven by server capabilities; hiding a menu is not a source boundary.
4. Public history must start from a clean root and must not contain enterprise source or private configuration.
5. Every private release records the public core revision and enterprise revision used to build it.

The standard startup path runs the public migration registry against
`seaql_migrations`. An enterprise distribution runs its own registry after the
public registry on the same migration connection, then opens the application
pool. This keeps migration ordering and SQLite write-lock handling inside the
database boundary while preventing enterprise migration records from entering
the public registry.

## First migration candidates

The machine-readable inventory in `docs/public-core-extraction.json` is the
source of truth for the first extraction order. Run
`python tools/ci/verify-public-core-boundary.py --mode inventory` from this
checkout to review the current footprint. The command does not modify files or
run a build. Once a public checkout has been assembled, run the same checker
with `--mode public`; it fails closed if any listed enterprise path remains.

The first pass is intentionally staged. Domain values and migration ownership
must move before repositories and services, then HTTP routes, and finally the
two frontend feature trees. Shared contracts stay in the public core and are
reviewed independently. A path is not considered extracted merely because the
frontend hides its navigation entry.

The first enterprise extraction should move these implementation areas together:

- `crates/af-account/src/organization_*` for enterprise login and invitation flows.
- `crates/af-admin/src/organization*.rs` and `crates/af-admin/src/platform_organization*.rs` for enterprise services.
- `crates/af-domain/src/organization*.rs` for enterprise policies and value objects.
- `crates/af-db/src/organization*.rs`, `crates/af-db/src/organization/`, and the organization migration modules for persistence.
- `crates/af-cache/src/organization_*` for SCIM and SSO rate limiting.
- `crates/af-http/src/organization*.rs`, `crates/af-http/src/organizations.rs`, and `crates/af-http/src/scim.rs` for enterprise endpoints.
- `web/src/features/organization*`, `web/src/features/platform-organizations`, and their `web-next` counterparts for enterprise screens.

The public core keeps shared principals, authentication, model gateway protocols,
usage accounting, and the extension contract in `af-http`. The initial contract
is `HttpExtension`: an enterprise implementation returns authenticated public,
management, and webhook routers plus a stable capability descriptor. The legacy
in-tree routes remain enabled during migration; they can be removed only after
the private enterprise implementation supplies the same route and capability
coverage.

The capability catalog is available at `GET /api/extensions`. It contains only
extension identifiers, display names, and feature keys, so both built-in
frontends can decide which navigation and controls to render without receiving
private service configuration. `openapi_document_with_extensions` is the
composition hook for merging private API documents into the generated contract.

The server bootstrap exposes the same boundary through
`Bootstrap::with_http_extensions`. A distribution constructs its
`HttpExtensions` registry before initializing telemetry and passes it into the
bootstrap; the registry is carried through the staged startup and consumed when
the final router is built. The default binary leaves the registry empty. The
legacy in-tree enterprise implementation is still linked during extraction;
an empty registry alone does not make the current build a community edition.

Database migrations use a separate startup hook,
`Bootstrap::with_database_migration_extension`. The extension implements
`af_db::DatabaseMigrationExtension`, normally by wrapping its own
`MigratorTrait` with `af_db::MigratorExtension`. The bootstrap runs the public
registry first and the extension registry second on the same migration
connection, including when a dedicated migration connection is configured.

## Runtime assembly

`Bootstrap<DatabaseReady>::init_authentication_with_organization` accepts a
distribution-owned factory for `af_server::OrganizationRuntimeServices`. It
calls the factory once, after migrations and shared authentication services
are ready, and before relay, billing, background tasks, and HTTP routes start.
Any factory error aborts startup without falling back to the built-in services.
Private factories can wrap their own startup errors with
`BootstrapError::OrganizationRuntime`; its display text omits the nested details.

The factory receives an `OrganizationRuntimeContext` with the application
database pool, validated configuration, session and profile services, optional
Passkey authentication, the system secret cipher, shared request-log repositories,
and the initialized SCIM rate-limit store. Enterprise SSO must reuse these
authentication instances so that public and enterprise flows use the same
session, profile, and credential policies.

The default `init_authentication` entry point delegates to
`OrganizationRuntimeServices::from_database`. The built-in enterprise assembly
now lives in `crates/af-server/src/organization_runtime.rs`; a distribution can
replace the factory or use it during a staged migration. The service bundle
still contains concrete organization repositories and an SSO transaction service,
and enterprise background jobs still use in-tree implementations. The bundle now
also exposes `OrganizationBackgroundTaskRegistrar`: the default registrar keeps
approval timeout handling, SSO domain re-verification, and SSO login transaction
cleanup unchanged, while a distribution can replace or omit those registrations.
These remaining concrete service dependencies must be extracted before publishing
the public core.

The same bundle supplies `organization_contract_price_source` and
`organization_service_account_audit_sink` through the public `af-billing` traits.
The shared billing startup clones those injected instances. It does not construct
enterprise pricing or audit repositories. The transitional database adapters live
in `crates/af-server/src/organization_runtime/billing.rs` and belong to the
enterprise extraction inventory. Pricing resolution errors still abort request
pricing, and audit errors still retain the usage record for queue retry.

## Billing contract snapshots

The request billing layer consumes `af_domain::BillingContractPriceSnapshot`, a
validated immutable value containing the owner, price record, version, and five
token price components. It is a shared billing contract and does not depend on
SeaORM, database entities, or an enterprise repository. `af-billing` exposes
only this value through `ContractPriceSource`; the database repository converts
its private record into the value at the adapter boundary. Quota persistence
stores and restores the snapshot fields without exposing the repository type to
the public billing crate.

The snapshot uses the public `BillingContractPriceId`. The enterprise repository
retains `OrganizationContractPriceId` for its management and approval contracts,
and converts the validated numeric ID when producing the billing snapshot. This
does not change reservation columns or historical replay values.

This keeps public pricing and lifecycle code independent from the enterprise
contract-price table while preserving owner checks, versioned pricing, and
idempotent reservation replay. A future enterprise distribution can provide a
different `ContractPriceSource` without adding its database types to `af-billing`.

## Token authentication

The public `TokenAuthRepository` queries shared token, user, and group state. It
does not join organization, membership, team, department, or entitlement tables.
Organization tokens are validated through `OrganizationTokenAuthValidator`,
injected by `OrganizationRuntimeServices` during authentication startup.

The validator receives `OrganizationTokenAuthContext`: token, user, organization,
membership, and optional team and department IDs. The context contains neither
the API key nor its hash, and no enterprise configuration. The public repository
checks that the returned organization principal matches every ownership ID in
that context before accepting it.

Missing validators, rejected enterprise state, and validator errors never
produce a personal principal. Shared token state and enterprise validation use
the same lookup deadline. Personal tokens do not invoke the enterprise validator.

`DatabaseOrganizationTokenAuthValidator` is the transitional in-tree
implementation in `crates/af-server/src/organization_runtime/token_auth.rs`.
It owns membership, entitlement, team, department, and organization key capacity
policy. The enterprise-owned `OrganizationTokenAuthRepository` in `af-db`
rechecks persisted token ownership and returns structured database facts; it
does not decide whether an organization may consume gateway services. ORM
entities remain private to `af-db`.

Both the runtime validator and its database reader remain part of the enterprise
extraction inventory and must move to the private enterprise repository before
publication. Moving the policy into the runtime is an intermediate step, not a
completed source extraction. The validator contract and shared principal IDs
remain public. Integration coverage belongs to the enterprise runtime; public
authentication tests exercise the injected validator contract independently.

## Extension persistence

`DatabasePool::extension_connection` lends the shared connection to
extension-owned persistence code as `ConnectionTrait + TransactionTrait`.
Extensions can execute structured queries and transactions after migrations
without opening a second pool or copying public ORM entities. The concrete
connection type and pool lifecycle remain owned by `af-db`; the borrowed
interface cannot close or replace the pool.

This is a persistence boundary, not an application service API. Extension
repositories must enforce their own validation, ownership checks, query
deadlines, and error redaction. The token authentication reader uses the
public authentication deadline and must not introduce an independent one.
Shared user, token, and group writes continue through the core repositories.
