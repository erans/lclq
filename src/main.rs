use lclq::cli::{Cli, commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    // Parse CLI arguments
    let cli = Cli::parse_args();

    // Execute the command
    commands::execute_command(cli.command).await?;

    Ok(())
}
