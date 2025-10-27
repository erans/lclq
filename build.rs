fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::Path;
    use std::process::Command;

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

    // Capture git information for version output
    // Get git commit hash (short)
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Check if working directory is dirty
    let git_dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(!output.stdout.is_empty())
            } else {
                None
            }
        })
        .unwrap_or(false);

    // Get build timestamp
    let build_timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Get build profile
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    // Set environment variables for use in the binary
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=GIT_DIRTY={}", if git_dirty { "dirty" } else { "clean" });
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", build_timestamp);
    println!("cargo:rustc-env=BUILD_PROFILE={}", profile);

    // Rerun if git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    Ok(())
}
