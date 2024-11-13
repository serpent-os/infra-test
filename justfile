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
	@docker build . -t serpentos/{{target}}-legacy --target {{target}} -f Dockerfile-legacy  

# Build docker containers
build: (docker-build-legacy "summit") (docker-build-legacy "avalanche") (docker-build "vessel")

# Bring up docker containers
up: build
	docker compose up
