fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;

    // Create the output directory if it doesn't exist
    let out_dir = "src/pubsub/generated";
    if !Path::new(out_dir).exists() {
        std::fs::create_dir_all(out_dir)?;
    }

    // Compile the Pub/Sub Protocol Buffer definitions
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .out_dir(out_dir)
        .compile_protos(&["proto/google/pubsub/v1/pubsub.proto"], &["proto"])?;

    // Tell cargo to rerun this build script if the proto file changes
    println!("cargo:rerun-if-changed=proto/google/pubsub/v1/pubsub.proto");
    println!("cargo:rerun-if-changed=proto");

    Ok(())
}
