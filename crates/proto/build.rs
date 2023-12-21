fn main() {
    tonic_build::compile_protos("proto/collectable.proto").unwrap();
    tonic_build::configure()
        .compile(
            &["proto/service/auth.proto", "proto/service/vessel.proto"],
            &["proto"],
        )
        .unwrap();
}
