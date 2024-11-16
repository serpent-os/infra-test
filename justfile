export MY_UID := `id -u`
export MY_GID := `id -g`
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
build: (docker-build-legacy "summit") (docker-build-legacy "avalanche") (docker-build-legacy "vessel")

# Bring up docker containers
up: build
	docker compose up --wait --renew-anon-volumes

# Bring down docker containers
down:
	docker compose down -v

# Bring up test environment and bootstrap
bootstrap: up
	#!/usr/bin/env bash
	./test/legacy/bootstrap.sh
	docker compose logs -f
	docker compose down -v
