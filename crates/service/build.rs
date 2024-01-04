fn main() {
    tonic_build::compile_protos("proto/google/protobuf/empty.proto").unwrap();
    tonic_build::configure()
        .compile(
            &[
                "proto/collectable.proto",
                "proto/endpoint.proto",
                "proto/vessel.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
