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
    TARGETS="vessel"
    for target in "${TARGETS}"
    do
      cargo build --release -p "$target"
      cp "/tmp/target/release/$target" /
    done
EOT

FROM alpine:3.20 AS vessel
VOLUME /state
EXPOSE 5003
WORKDIR /app
COPY --from=rust-builder /vessel .
CMD ["/app/vessel", "0.0.0.0", "--port", "5003", "--root", "/state"]
