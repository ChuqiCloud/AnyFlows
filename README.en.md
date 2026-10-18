# AnyFlows

[简体中文](README.md) · [Documentation](https://github.com/ChuqiCloud/AnyFlows/blob/main/docs/README.md) · [Releases](https://github.com/ChuqiCloud/AnyFlows/releases)

AnyFlows is a self-hosted AI API gateway. It connects multiple model providers through compatible APIs and provides channel routing, API keys, quotas, usage accounting, request logs, and two built-in administration frontends.

This repository contains the community core and can run independently. Private enterprise extensions are maintained separately.

## Installation

Binary packages are the recommended deployment method for Linux servers running systemd. They include both frontends and do not require Rust or Node.js on the server. Download the archive and matching SHA-256 file listed on the [Releases](https://github.com/ChuqiCloud/AnyFlows/releases) page. Beta releases are intended for testing and deployment validation.

After downloading an archive and its matching SHA-256 file, replace `VERSION` in `PACKAGE` with the downloaded version, including any prerelease suffix, and run:

```bash
PACKAGE='anyflows-vVERSION-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
sudo ./install.sh install
sudo ./install.sh status
```

The installer creates a service account, generates keys, and starts with SQLite. The application listens on `127.0.0.1:8080`. Complete initial administrator setup through an SSH tunnel before exposing the service through an HTTPS reverse proxy. There is no default administrator password.

The documentation is maintained primarily in Chinese:

- [Binary installation and first login](docs/deployment-and-configuration.md)
- [Multi-node deployment](docs/multi-node-deployment.md)
- [Docker Compose source deployment](docs/docker-compose.md)
- [Configuration](docs/configuration.md)
- [Backups, upgrades, and rollback](docs/migrations-and-release.md)

## Contributing

Open pull requests against `main`. See [CONTRIBUTING.md](CONTRIBUTING.md) for development tools and checks. Community contributors do not need private repository access.

## License

[GNU AGPL v3](LICENSE).
