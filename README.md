# Infra Test

Test bed for infra in rust

## Testing

- Run summit

```sh
cargo run -p summit -- --root crates/summit --config crates/summit/config.local.toml
```

- Add the summit public key from log line `keypair generated: ..` to `crates/avalanche/config.local.toml`

- Run avalanche

```sh
cargo run -p avalanche -- --root crates/avalanche --config crates/avalanche/config.local.toml
```

- Run CLI to accept avalanche enrollment request

```sh
cargo run -p cli
```

- Hit summit REST API to see added avalanche endpoint

```sh
curl -s -H 'content-type: application/json' 127.0.0.1:5000/api/v1/endpoints | jq
```
