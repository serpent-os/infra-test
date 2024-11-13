export MY_UID := `id -u`
export MY_GID := `id -g`
export COMPOSE_FILE := "./test/docker-compose.yaml"

[private]
help:
	@just --list

[private]
docker-build target:
	@docker build . -t serpentos/{{target}} --target {{target}} --no-cache

# Build docker containers
build: (docker-build "summit") (docker-build "vessel")

# Bring up docker containers
up: build
	docker compose up
