fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile the Pub/Sub Protocol Buffer definitions
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir("src/pubsub/generated")
        .compile_protos(&["proto/google/pubsub/v1/pubsub.proto"], &["proto"])?;

    // Tell cargo to rerun this build script if the proto file changes
    println!("cargo:rerun-if-changed=proto/google/pubsub/v1/pubsub.proto");

    Ok(())
}
