# Infra Test

Test bed for infra in rust

## Run

Run auth service

```sh
cargo run -p auth -- -p 5001
```

In another terminal, run summit

```sh
cargo run -p summit -- -p 5000 --auth "http://127.0.0.1:5001"
```

In another terminal, test w/ curl

```
curl -X POST --data '{"username":"myuser","password":"superdupersecretpassword"}' -H 'content-type: application/json' 127.0.0.1:5000/api/account/authenticate
```
