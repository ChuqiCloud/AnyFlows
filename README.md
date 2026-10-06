# AnyFlows

AnyFlows is a self-hosted API gateway for connecting applications to multiple AI model providers through one compatible interface. It provides channel management, model routing, API keys, usage accounting, request logs, rate limits, and an administration console.

The public repository contains the community core. Enterprise capabilities are maintained separately and are assembled by the distribution repository.

## Features

- OpenAI-compatible chat, responses, embeddings, audio, image, rerank, and model endpoints
- Multiple upstream providers with channel health checks and routing policies
- API key management, groups, quotas, pricing, usage records, and request logs
- PostgreSQL, MySQL, and SQLite support
- Redis-backed rate limiting and optional cache integrations
- React administration consoles, including the classic and next-generation frontends
- Extension contracts for enterprise routes, migrations, authentication, billing, and background tasks

## Technology

- Rust, Axum, Tokio, and SeaORM
- React, TypeScript, Tailwind CSS, and TanStack Router/Query
- PostgreSQL, MySQL, or SQLite
- Optional Redis

## Repository layout

This repository is the public core. Private enterprise features and release assembly are maintained in separate repositories and are not required for community development.

The main crates are:

| Crate | Responsibility |
| --- | --- |
| `af-server` | Startup, runtime wiring, and graceful shutdown |
| `af-http` | HTTP routing, middleware, and API contracts |
| `af-relay` | Request forwarding and provider routing |
| `af-billing` | Pricing, usage accounting, and quota integration |
| `af-account` | Authentication, sessions, and user accounts |
| `af-admin` | Shared administration services |
| `af-db` | Database repositories and migrations |
| `af-domain` | Shared domain types and contracts |
| `af-adapter` / `af-protocol` | Provider adapters and protocol translation |

Enterprise implementations use the extension contracts exposed by the public core. They can provide their own routes, migrations, authentication validators, billing sources, and background jobs without copying public ORM entities or changing the public router.

## Development

The repository pins its Rust toolchain in `rust-toolchain.toml`. Node.js and pnpm are required for frontend work.

```bash
cargo build --workspace
cargo test --workspace --all-features --locked
corepack pnpm@10.33.0 --dir web install --frozen-lockfile
corepack pnpm@10.33.0 --dir web check
```

The CI workflow is the authoritative validation environment. It runs formatting, dependency boundary checks, database tests, Clippy, Rust tests, frontend checks, and release checks.

## Running locally

Create a configuration from `.env.example`, then start the server with the normal Rust tooling or use the provided deployment scripts. The default service listens on `127.0.0.1:8080`.

```bash
cargo run --package af-server
curl http://127.0.0.1:8080/healthz
```

For production deployments, use the Docker Compose or systemd assets under `scripts/deploy` and place a reverse proxy in front of the local listener.

## API explorer

When enabled by the server, `/api` provides an API directory and online request editor. The catalog is filtered by the current session, so public, user, and administrator views expose only the operations that the caller may use.

## Contributing

Create a feature branch from `develop`, keep changes focused, and open a pull request with the relevant validation details. Do not commit credentials, generated secrets, local build output, or private enterprise source.

See [`docs/repository-boundary.md`](docs/repository-boundary.md) for the public and enterprise extension boundary.
The reproducible public checkout procedure is documented in
[`docs/public-core-export.md`](docs/public-core-export.md).

## License

See the repository license file for the terms that apply to this source tree.
