# Infra

[![docs](https://img.shields.io/badge/docs-passing-brightgreen)](https://serpent-os.github.io/infra-test/)

SerpentOS service infrastructure

## Prerequisites

- Infra can be brought up via `docker-compose`.
- `just` is used as a runner tool to streamline this.

```sh
# on solus:
eopkg it docker docker-compose just
sudo usermod -a -G docker
sudo systemctl reboot
```

## Testing

```sh
# Will build docker images and bring up `test/docker-compose.yaml`
just up
```
