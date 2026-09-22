# 第一阶段只负责构建前端静态资源。
FROM node:24-bookworm-slim AS frontend

WORKDIR /workspace
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./web/
RUN corepack pnpm@10.33.0 --dir web install --frozen-lockfile
COPY web-next/package.json web-next/pnpm-lock.yaml ./web-next/
RUN corepack pnpm@10.33.0 --dir web-next install --frozen-lockfile
COPY web-next ./web-next
RUN corepack pnpm@10.33.0 --dir web-next build
COPY web ./web
RUN corepack pnpm@10.33.0 --dir web build

# 第二阶段编译带前端内嵌资源的 Linux amd64 单二进制。
FROM rust:1.97.1-bookworm AS builder

WORKDIR /workspace
ENV CARGO_BUILD_JOBS=1 \
    CARGO_INCREMENTAL=0
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml clippy.toml deny.toml ./
COPY .cargo ./.cargo
COPY crates ./crates
COPY tools ./tools
COPY --from=frontend /workspace/web/dist ./web/dist
COPY --from=frontend /workspace/web-next/dist ./web-next/dist
RUN cargo build --release --locked --package af-server

# 运行时保持非 root、只读根文件系统和最小依赖。
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 --shell /usr/sbin/nologin anyflows

WORKDIR /app
COPY --from=builder /workspace/target/release/af-server /usr/local/bin/anyflows
RUN mkdir --mode=0700 /app/data \
    && chown --recursive anyflows:anyflows /app

USER anyflows
ENV AF_SERVER__BIND=127.0.0.1:8080 \
    AF_BILLING__WAL_DIRECTORY=/app/data/billing-wal
VOLUME ["/app/data"]
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=10s --timeout=3s --start-period=15s --retries=10 \
    CMD wget --no-verbose --tries=1 --spider http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/anyflows"]
