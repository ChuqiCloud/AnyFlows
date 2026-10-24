# AnyFlows

[简体中文](README.md) · [Documentation](docs/README.md) · [Releases](https://github.com/ChuqiCloud/AnyFlows/releases)

AnyFlows is a self-hosted AI API gateway. It connects multiple model providers through one service, with API keys, model routing, quotas, usage accounting, and request logs.

## Features

- OpenAI Chat, OpenAI Responses, Anthropic Messages, and Gemini GenerateContent, with request and response conversion between protocols.
- Streaming, tool calls, and multi-turn conversations, preserving supported history and usage data.
- Channels, credential pools, account pools, and routing based on model capabilities and channel status.
- Users, groups, model permissions, pricing, and quotas.
- Classic and Next administration frontends, with support for external static templates.
- SQLite, PostgreSQL, and MySQL, with optional Redis for shared state.

This repository contains the standalone community edition.

## Acknowledgments & Community Sponsors

Thanks to the following partners for supporting AnyFlows.

<!-- Brand logos belong to their respective owners. Source: https://acmecloud.cn/partners -->
<table>
  <tr>
    <td align="center" width="50%">
      <a href="https://chuqiyun.com/">
        <img src="docs/assets/sponsors/chuqicloud.png" alt="Chuqi Cloud (初七云)" height="48">
      </a>
      <p><strong><a href="https://chuqiyun.com/">Chuqi Cloud</a> · Cloud Computing Support</strong></p>
      <p>Cloud computing services and infrastructure support for AnyFlows.</p>
    </td>
    <td align="center" width="50%">
      <a href="https://www.mefrp.com/">
        <picture>
          <source media="(prefers-color-scheme: dark)" srcset="docs/assets/sponsors/mefrp-dark.svg">
          <source media="(prefers-color-scheme: light)" srcset="docs/assets/sponsors/mefrp-light.svg">
          <img src="docs/assets/sponsors/mefrp-light.svg" alt="ME Frp (幻缘映射)" height="48">
        </picture>
      </a>
      <p><strong><a href="https://www.mefrp.com/">ME Frp</a> · Community Sponsor</strong></p>
      <p>A free tunneling service for remote access, multiplayer gaming, and development.</p>
    </td>
  </tr>
</table>

## Installation

Community binary packages currently support Linux amd64 and include both frontends. Rust and Node.js are not required at runtime. Download the archive and matching `.sha256` file from [Releases](https://github.com/ChuqiCloud/AnyFlows/releases). Beta releases are intended for testing.

Replace `VERSION` with the downloaded version, including its prerelease suffix, then verify and extract the archive:

```bash
PACKAGE='anyflows-vVERSION-linux-amd64'
sha256sum --check "$PACKAGE.tar.gz.sha256"
tar -xzf "$PACKAGE.tar.gz"
cd "$PACKAGE"
```

Follow [binary deployment](docs/deployment-and-configuration.md) to create a service account, configure the database and keys, and start the application with systemd. A single node can use SQLite. The default address is `127.0.0.1:8080`. Create the administrator through an SSH tunnel, then configure an HTTPS reverse proxy.

For other deployment options, see [Docker Compose](docs/docker-compose.md) and [multi-node deployment](docs/multi-node-deployment.md).

## Getting started

1. Complete initialization and create the administrator in the console.
2. Add channels and credentials, then configure models, routing, and pricing.
3. Create a user API key and configure the client with the service URL, key, and model name.
4. Send a request and review the result and usage in the request logs. The `/api` page provides an API directory and request debugging.

OpenAI-compatible clients typically use `https://api.example.com/v1` as the base URL. Clients that append `/v1` automatically should use the site root. See [conversation compatibility](docs/conversation-compatibility.md) for protocol details.

## Documentation

The documentation is maintained primarily in Chinese:

- [Binary deployment and first login](docs/deployment-and-configuration.md)
- [Multi-node deployment](docs/multi-node-deployment.md)
- [Docker Compose source deployment](docs/docker-compose.md)
- [Configuration](docs/configuration.md)
- [Backups, upgrades, and rollback](docs/migrations-and-release.md)

## Contributing

Open pull requests against `main`. See [CONTRIBUTING.md](CONTRIBUTING.md) for development tools and checks.

## License

[GNU AGPL v3](LICENSE).
