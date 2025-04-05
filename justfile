export COMPOSE_FILE := "./test/docker-compose.yaml"

[private]
help:
	@just --list

[private]
docker-build target:
	@docker build . -t serpentos/{{target}} --target {{target}}

[private]
docker-build-legacy target:
	@docker build . -t serpentos/{{target}}:legacy --target {{target}} -f test/legacy/Dockerfile

# Build docker containers
build: (docker-build "summit") (docker-build "avalanche") (docker-build "vessel")

# Bring up docker containers
up *ARGS: build
	docker compose up --wait {{ARGS}}

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
