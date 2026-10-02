# Repository Boundary

This document defines the first extraction boundary for the public core, the private enterprise extensions, and the private distribution repository.

## Public core

The public repository owns model gateway protocols, provider routing, credentials and channels, usage accounting, request logs, shared authentication, common administration, and the two shared console shells.

## Enterprise extensions

The private enterprise repository owns organizations, memberships, departments, enterprise wallets, approvals, SSO, SCIM, enterprise verification, enterprise plans, enterprise service accounts, and the corresponding API and console pages.

## Distribution

The private distribution repository owns the selected public core revision, the compatible enterprise revision, release metadata, deployment overlays, and packaging configuration. It must not contain an independent copy of either codebase.

## Extraction rules

1. Enterprise database migrations remain outside the public migration chain.
2. Enterprise routes and OpenAPI documents are registered through an extension boundary.
3. Frontend navigation is driven by server capabilities; hiding a menu is not a source boundary.
4. Public history must start from a clean root and must not contain enterprise source or private configuration.
5. Every private release records the public core revision and enterprise revision used to build it.
