FROM node:24.15.0-bookworm-slim@sha256:4e6b70dd6cbfc88c8157ba19aa3d9f9cce6ba4703576d55459e45efcbc9c5f5d AS node-runtime

FROM rust:1.97.1-bookworm@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# 两个基础镜像均为多架构清单，当前 Runner 会自动选择对应的原生平台。
COPY --from=node-runtime /usr/local/ /usr/local/

RUN rustc --version --verbose \
    && cargo --version \
    && node --version \
    && corepack --version \
    && tar --version \
    && sha256sum --version
