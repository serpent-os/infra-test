FROM rust:alpine3.20 AS rust-builder
ENV RUSTUP_HOME="/usr/local/rustup" \
    CARGO_HOME="/usr/local/cargo" \
    CARGO_TARGET_DIR="/tmp/target"
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static git
WORKDIR /src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git <<"EOT" /bin/sh
    git clone https://github.com/serpent-os/tools /tools
    cd /tools
    git checkout fix/run-in-docker
    cargo install --path ./boulder
EOT
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/tmp/target \
    --mount=type=bind,target=/src <<"EOT" /bin/sh
    for target in vessel summit avalanche
    do
      cargo build -p "$target"
      cp "/tmp/target/debug/$target" /
    done
EOT

FROM alpine:3.20 AS summit
WORKDIR /app
COPY --from=rust-builder /summit .
VOLUME /app/state
VOLUME /app/config.toml
EXPOSE 5000
ENTRYPOINT ["/app/summit"]
CMD ["0.0.0.0", "--port", "5000", "--root", "/app"]

FROM alpine:3.20 AS vessel
WORKDIR /app
COPY --from=rust-builder /vessel .
VOLUME /app/state
VOLUME /app/config.toml
VOLUME /import
EXPOSE 5001
ENTRYPOINT ["/app/vessel"]
CMD ["0.0.0.0", "--port", "5001", "--root", "/app", "--import", "/import"]

FROM alpine:3.20 AS avalanche
WORKDIR /app
RUN apk add --no-cache sudo git
COPY --from=rust-builder /avalanche .
COPY --from=rust-builder /usr/local/cargo/bin/boulder /usr/bin/boulder
COPY --from=rust-builder /tools/boulder/data/macros /usr/share/boulder/macros
VOLUME /app/state
VOLUME /app/config.toml
EXPOSE 5002
ENTRYPOINT ["/app/avalanche"]
CMD ["0.0.0.0", "--port", "5002", "--root", "/app"]
