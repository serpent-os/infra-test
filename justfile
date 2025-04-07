export COMPOSE_FILE := "./test/docker-compose.yaml"

[private]
help:
	@just --list

[private]
docker-build target profile:
	@docker build . -t serpentos/{{target}}:{{profile}} --target {{target}} --build-arg RUST_PROFILE={{profile}}

[private]
docker-build-legacy target:
	@docker build . -t serpentos/{{target}}:legacy --target {{target}} -f test/legacy/Dockerfile

# Build docker containers
build profile="dev": (docker-build "summit" profile) (docker-build "avalanche" profile) (docker-build "vessel" profile)

# Bring up docker containers
up *ARGS: (_up "dev" ARGS)

# Bring up docker containers in release mode
up-release *ARGS: (_up "release" ARGS)

_up profile *ARGS: (build profile)
	RUST_PROFILE={{profile}} docker compose up --wait {{ARGS}}

# Follow logs of docker containers
logs *ARGS:
	docker compose logs --follow {{ARGS}}

# Bring down docker containers
down *ARGS:
	docker compose down -v {{ARGS}}

# Bring up test environment and bootstrap
bootstrap: up
	#!/usr/bin/env bash
	./test/legacy/bootstrap.sh
	docker compose logs -f
	docker compose down -v
