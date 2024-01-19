compose-file := "./test/docker-compose.yaml"
uid := `id -u`
gid := `id -g`

[private]
help:
	@just --list

[private]
docker-build target:
	@docker build . -t serpentos/{{target}} --target {{target}}

# Build docker containers
build: (docker-build "summit") (docker-build "avalanche")

# Bring up docker containers
up: build
	@COMPOSE_FILE={{compose-file}} MY_UID={{uid}} MY_GID={{gid}} docker-compose up	
