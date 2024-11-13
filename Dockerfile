FROM rust:alpine3.20 AS rust-builder
ENV RUSTUP_HOME="/usr/local/rustup" \
    CARGO_HOME="/usr/local/cargo" \
    CARGO_TARGET_DIR="/tmp/target"
WORKDIR /src
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/tmp/target \
    --mount=type=bind,target=/src <<"EOT" /bin/sh
    for target in vessel summit
    do
      cargo build -p "$target"
      cp "/tmp/target/debug/$target" /
    done
EOT

FROM alpine:3.20 AS summit
VOLUME /state
EXPOSE 5000
WORKDIR /app
COPY --from=rust-builder /summit .
CMD ["/app/summit", "0.0.0.0", "--port", "5000", "--root", "/state"]

FROM alpine:3.20 AS vessel
VOLUME /state
EXPOSE 5001
WORKDIR /app
COPY --from=rust-builder /vessel .
CMD ["/app/vessel", "0.0.0.0", "--port", "5001", "--root", "/state"]
