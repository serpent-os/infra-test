FROM debian:bookworm AS builder
RUN apt update
RUN apt install -y \
	build-essential \
	curl \
	wget \
	git \
	libcurl4-openssl-dev \
	libdbus-1-dev \
	liblmdb-dev \
	libsodium-dev \
	libssl-dev \
	libxml2-dev \
	libxxhash-dev \
	libzstd-dev \
	zlib1g-dev
RUN mkdir -p ~/dlang && wget https://dlang.org/install.sh -O ~/dlang/install.sh
RUN chmod +x ~/dlang/install.sh
RUN ~/dlang/install.sh install ldc-1.32.2
RUN git clone https://github.com/serpent-os/libmoss.git
RUN git clone https://github.com/serpent-os/moss-service.git
RUN git clone https://github.com/serpent-os/summit.git && cd summit && git checkout feat/build-remotes
RUN git clone https://github.com/serpent-os/vessel.git
RUN git clone https://github.com/serpent-os/avalanche.git
RUN --mount=type=cache,target=/root/.dub <<"EOT" /bin/bash
	source ~/dlang/ldc-1.32.2/activate
	cd summit
	dub build --parallel
EOT
RUN --mount=type=cache,target=/root/.dub <<"EOT" /bin/bash
	source ~/dlang/ldc-1.32.2/activate
	cd vessel
	dub build --parallel
EOT
RUN --mount=type=cache,target=/root/.dub <<"EOT" /bin/bash
	source ~/dlang/ldc-1.32.2/activate
	cd avalanche
	dub build --parallel
EOT
# Install boulder
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN git clone https://github.com/serpent-os/tools.git && cd tools
RUN ~/.cargo/bin/cargo install --path tools/boulder

FROM debian:bookworm-slim AS base
RUN apt update
RUN apt install -y \
	git \
	sudo \
	iproute2 \
	libcurl4 \
	libdbus-1-3 \
	liblmdb0 \
	libsodium23 \
	libssl3 \
	libxml2 \
	libxxhash0 \
	libzstd1 \
	zlib1g
RUN rm -rf /var/cache/apt/archives /var/lib/apt/lists/*

FROM base as summit
COPY --from=builder /summit /app
RUN rm /app/seed.json
VOLUME /app/state
VOLUME /app/seed.json
EXPOSE 5000
WORKDIR /app
CMD ["/app/summit", "-a", "0.0.0.0", "-p", "5000"]

FROM base as vessel
COPY --from=builder /vessel /app
VOLUME /app/state
EXPOSE 5001
WORKDIR /app
CMD ["/app/vessel", "-a", "0.0.0.0", "-p", "5001"]

FROM base as avalanche
COPY --from=builder /avalanche /app
COPY --from=builder /root/.cargo/bin/boulder /usr/bin/boulder
COPY --from=builder /tools/boulder/data/macros /usr/share/boulder/macros
RUN mkdir -p /app/public
RUN mkdir -p /var/cache/boulder
VOLUME /app/state
EXPOSE 5002
WORKDIR /app
CMD ["/app/avalanche", "-a", "0.0.0.0", "-p", "5002"]
