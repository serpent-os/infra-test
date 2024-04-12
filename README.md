# Infra Test

Test bed for infra in rust

## Testing

- Test infra can be brought up via `docker-compose`. `just` is used as a runner tool to streamline this.

```sh
# Will build docker images and bring up `test/docker-compose.yaml`
just up
```

- Run CLI with test admin key to accept initial pairing

```sh
cargo run -p cli ./test/admin.pem
```

- Access frontend at `http://localhost:5000` and you should see this endpoint listed

- Hit summit REST API to see added avalanche endpoint

```sh
curl -s -H 'content-type: application/json' localhost:5000/api/v1/endpoints | jq
```

#### Frontend

- Install pnpm
- Setup and run dev

```sh
cd crates/summit/frontend
# Use pinned node version
pnpm env use --global $(cat .nvmrc)
# install deps
pnpm install
# run dev server (vite is setup to proxy api requests to backend in dev)
pnpm dev
```
