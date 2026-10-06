FROM rust:1.97.1-bookworm@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

ARG CARGO_NEXTEST_VERSION=0.9.140
ARG CARGO_NEXTEST_SHA256=4ee9aaa0d0171a985a5d0eb735b87355894c1c455972e9674fb9fdbd1387c9a3
ARG CARGO_DENY_VERSION=0.20.2
ARG CARGO_DENY_SHA256=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f

# 固定官方镜像摘要，基础镜像已提供下载和校验所需工具。
RUN set -eux; \
    rustup component add rustfmt clippy; \
    rustup toolchain install 1.95.0 --profile minimal

# 固定版本并校验发布包，避免上游 latest 漂移或下载内容被静默替换。
RUN set -eux; \
    archive="/tmp/cargo-nextest.tar.gz"; \
    curl --fail --location --retry 5 --retry-all-errors \
        "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${CARGO_NEXTEST_VERSION}/cargo-nextest-${CARGO_NEXTEST_VERSION}-x86_64-unknown-linux-gnu.tar.gz" \
        --output "${archive}"; \
    echo "${CARGO_NEXTEST_SHA256}  ${archive}" | sha256sum --check --strict; \
    tar --extract --gzip --file="${archive}" --directory=/usr/local/cargo/bin cargo-nextest; \
    chmod 0755 /usr/local/cargo/bin/cargo-nextest; \
    rm -f "${archive}"

RUN set -eux; \
    archive="/tmp/cargo-deny.tar.gz"; \
    extract_dir="/tmp/cargo-deny"; \
    curl --fail --location --retry 5 --retry-all-errors \
        "https://github.com/EmbarkStudios/cargo-deny/releases/download/${CARGO_DENY_VERSION}/cargo-deny-${CARGO_DENY_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
        --output "${archive}"; \
    echo "${CARGO_DENY_SHA256}  ${archive}" | sha256sum --check --strict; \
    mkdir --parents "${extract_dir}"; \
    tar --extract --gzip --file="${archive}" --directory="${extract_dir}"; \
    install --mode=0755 \
        "${extract_dir}/cargo-deny-${CARGO_DENY_VERSION}-x86_64-unknown-linux-musl/cargo-deny" \
        /usr/local/cargo/bin/cargo-deny; \
    rm -rf "${archive}" "${extract_dir}"

RUN cargo nextest --version \
    && cargo deny --version \
    && cargo +1.95.0 --version
