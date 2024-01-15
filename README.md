# Infra Test

Test bed for infra in rust

## Testing

- Generate ED25519 private key and add it's encoded public key to `crates/summit/config.local.toml`

```
openssl genpkey -algorithm ED25519 -out admin.pem
openssl pkey -in admin2.pem -pubout -outform DER | tail -c 32 | base64 | tr -d '='
```

- Run summit

```sh
cargo run -p summit -- --root crates/summit --config crates/summit/config.local.toml
```

- Add the summit public key from log line `keypair generated: ..` to `crates/avalanche/config.local.toml`

- Run avalanche

```sh
cargo run -p avalanche -- --root crates/avalanche --config crates/avalanche/config.local.toml
```

- Run CLI with private key from above to accept avalanche enrollment request

```sh
cargo run -p cli ./admin.pem
```

- Hit summit REST API to see added avalanche endpoint

```sh
curl -s -H 'content-type: application/json' 127.0.0.1:5000/api/v1/endpoints | jq
```
