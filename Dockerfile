FROM rust:latest as rust-builder
ENV RUSTUP_HOME="/usr/local/rustup" \
    CARGO_HOME="/usr/local/cargo" \
    CARGO_TARGET_DIR="/tmp/target"
WORKDIR /src
RUN apt-get update && apt-get install -y protobuf-compiler
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/tmp/target \
    --mount=type=bind,target=/src <<"EOT" bash
    TARGETS=(summit avalanche)
    for target in "${TARGETS[@]}"
    do
      cargo build --release -p "$target"
      cp "/tmp/target/release/$target" /
    done
EOT

FROM node:18-slim AS node-builder
ENV PNPM_HOME="/pnpm" \
    PATH="$PNPM_HOME:$PATH"
COPY . /src
WORKDIR /src/crates/summit/frontend
RUN corepack enable
RUN --mount=type=cache,target=/pnpm/store <<"EOT" bash
    pnpm install --frozen-lockfile
    pnpm build
    cp -r build /assets
EOT

FROM debian:bullseye-slim as avalanche
VOLUME /state
EXPOSE 5002
WORKDIR /app
COPY --from=rust-builder /avalanche .
CMD ["/app/avalanche", "0.0.0.0", "--root", "/state"]

FROM debian:bullseye-slim as summit
VOLUME /state
EXPOSE 5000 5001
WORKDIR /app
COPY --from=rust-builder /summit .
COPY --from=node-builder /assets /assets
CMD ["/app/summit", "0.0.0.0", "--root", "/state", "--assets", "/assets"]
