fn main() {
    tonic_build::compile_protos("proto/google/protobuf/empty.proto").unwrap();
    tonic_build::configure()
        .type_attribute("account.TokenResponse", "#[allow(missing_docs)]")
        .compile(
            &[
                "proto/account.proto",
                "proto/collectable.proto",
                "proto/endpoint.proto",
                "proto/vessel.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
