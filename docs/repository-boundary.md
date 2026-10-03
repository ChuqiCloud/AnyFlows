# Repository Boundary

This document defines the first extraction boundary for the public core, the private enterprise extensions, and the private distribution repository.

## Public core

The public repository owns model gateway protocols, provider routing, credentials and channels, usage accounting, request logs, shared authentication, common administration, and the two shared console shells.

## Enterprise extensions

The private enterprise repository owns organizations, memberships, departments, enterprise wallets, approvals, SSO, SCIM, enterprise verification, enterprise plans, enterprise service accounts, and the corresponding API and console pages.

## Distribution

The private distribution repository owns the selected public core revision, the compatible enterprise revision, release metadata, deployment overlays, and packaging configuration. It must not contain an independent copy of either codebase.

## Extraction rules

1. Enterprise database migrations remain outside the public migration chain. The
   public `af-db` crate exposes `run_pending_migrations_with` and
   `connect_and_migrate_with` for an extension-owned `MigratorTrait`; the
   extension must override `migration_table_name` and use its own migration
   table.
2. Enterprise routes and OpenAPI documents are registered through an extension boundary.
   The built-in route composition accepts `af_http::OrganizationHttpServices` as one
   dependency bundle, so a distribution can assemble enterprise services in its own
   startup module without expanding the public router function signature.
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
