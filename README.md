# AnyFlows

AnyFlows is an open-source gateway for serving multiple AI model providers through one API.

It provides OpenAI-compatible endpoints, provider routing, usage accounting, access control, and an administration console for teams that need to operate model traffic from a single service.

## What is included

- Unified API endpoints for supported model protocols
- Provider and channel routing
- API key management and request accounting
- Usage logs and operational reporting
- PostgreSQL, MySQL, and SQLite support
- Rust backend with React-based web consoles

The public repository contains the community edition. Enterprise capabilities are maintained and distributed separately.

## Technology

- Rust, Axum, Tokio, and SeaORM
- React, TypeScript, and Tailwind CSS
- Redis for cache and coordination where enabled
- Docker Compose or Linux systemd deployment

## Project status

The public repository is being prepared for its first community source release. The repository layout and contribution workflow will be finalized together with that release.

## Contributing

Bug reports, documentation improvements, and focused code changes are welcome. Please open an issue before starting a large change so that the scope and API impact can be discussed first.

Pull requests should include a clear description, the affected area, and the validation performed. Do not include credentials, production configuration, private provider details, or customer data.

## Security

Do not report security issues in public issues. Use the repository's private security contact once it is published.

## License

The project license will be included before the first public source release.
