// Commands module
/// Start command implementation
pub mod start;

use crate::cli::{output::*, Commands, QueueCommands};
use anyhow::Context;
use serde::{Deserialize, Serialize};
use tabled::Tabled;

/// Health check response
#[derive(Debug, Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    backend_status: String,
}

/// Overall statistics response
#[derive(Debug, Serialize, Deserialize, Tabled)]
struct StatsResponse {
    total_queues: usize,
    total_messages: u64,
    total_in_flight: u64,
}

/// Queue summary for list
#[derive(Debug, Serialize, Deserialize, Tabled)]
struct QueueSummary {
    id: String,
    name: String,
    queue_type: String,
    available_messages: u64,
    in_flight_messages: u64,
}

/// Queue list response
#[derive(Debug, Serialize, Deserialize)]
struct QueueListResponse {
    queues: Vec<QueueSummary>,
}

/// Queue details response
#[derive(Debug, Serialize, Deserialize, Tabled)]
struct QueueDetailsResponse {
    id: String,
    name: String,
    queue_type: String,
    visibility_timeout: u32,
    message_retention_period: u32,
    max_message_size: usize,
    delay_seconds: u32,
    available_messages: u64,
    in_flight_messages: u64,
    #[tabled(display_with = "display_option")]
    dlq_target_queue_id: Option<String>,
    #[tabled(display_with = "display_option")]
    max_receive_count: Option<u32>,
}

/// Display function for Option types in tables
fn display_option<T: std::fmt::Display>(option: &Option<T>) -> String {
    option.as_ref().map(|v| v.to_string()).unwrap_or_else(|| "-".to_string())
}

/// Create queue request
#[derive(Debug, Serialize)]
struct CreateQueueRequest {
    name: String,
}

/// Execute a CLI command
pub async fn execute_command(command: Commands) -> anyhow::Result<()> {
    match command {
        Commands::Start {
            sqs_port,
            admin_port,
            metrics_port,
            backend,
            db_path,
        } => {
            start::execute(sqs_port, admin_port, metrics_port, backend, db_path).await
        }
        Commands::Queue(queue_cmd) => execute_queue_command(queue_cmd).await,
        Commands::Health { admin_url } => execute_health(admin_url).await,
        Commands::Stats { admin_url } => execute_stats(admin_url).await,
        Commands::Config => execute_config().await,
    }
}

async fn execute_queue_command(command: QueueCommands) -> anyhow::Result<()> {
    match command {
        QueueCommands::List { admin_url, format } => {
            let client = reqwest::Client::new();
            let url = format!("{}/queues", admin_url);

            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to connect to Admin API")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, error_text);
            }

            let queue_list: QueueListResponse = response
                .json()
                .await
                .context("Failed to parse response")?;

            let output_format = OutputFormat::parse(&format);
            print_list(&queue_list.queues, output_format)?;
            Ok(())
        }
        QueueCommands::Create { name, admin_url } => {
            let client = reqwest::Client::new();
            let url = format!("{}/queues", admin_url);

            let request = CreateQueueRequest { name: name.clone() };

            let response = client
                .post(&url)
                .json(&request)
                .send()
                .await
                .context("Failed to connect to Admin API")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, error_text);
            }

            let queue_details: QueueDetailsResponse = response
                .json()
                .await
                .context("Failed to parse response")?;

            print_success(&format!("Queue '{}' created successfully", name));
            println!("\nQueue Details:");
            print_output(&queue_details, OutputFormat::Table)?;
            Ok(())
        }
        QueueCommands::Delete { name, admin_url } => {
            let client = reqwest::Client::new();
            let url = format!("{}/queues/{}", admin_url, name);

            let response = client
                .delete(&url)
                .send()
                .await
                .context("Failed to connect to Admin API")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, error_text);
            }

            print_success(&format!("Queue '{}' deleted successfully", name));
            Ok(())
        }
        QueueCommands::Purge { name, admin_url } => {
            let client = reqwest::Client::new();
            let url = format!("{}/queues/{}/purge", admin_url, name);

            let response = client
                .post(&url)
                .send()
                .await
                .context("Failed to connect to Admin API")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, error_text);
            }

            print_success(&format!("Queue '{}' purged successfully", name));
            Ok(())
        }
        QueueCommands::Stats { name, admin_url } => {
            let client = reqwest::Client::new();
            let url = format!("{}/queues/{}", admin_url, name);

            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to connect to Admin API")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                anyhow::bail!("API error ({}): {}", status, error_text);
            }

            let queue_details: QueueDetailsResponse = response
                .json()
                .await
                .context("Failed to parse response")?;

            println!("Queue Statistics for '{}':", name);
            print_output(&queue_details, OutputFormat::Table)?;
            Ok(())
        }
    }
}

async fn execute_health(admin_url: String) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/health", admin_url);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to Admin API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("API error ({}): {}", status, error_text);
    }

    let health: HealthResponse = response
        .json()
        .await
        .context("Failed to parse response")?;

    if health.status == "healthy" {
        print_success(&format!("Server is {}", health.status));
        print_info(&format!("Backend: {}", health.backend_status));
    } else {
        print_error(&format!("Server is {}", health.status));
        print_warning(&format!("Backend: {}", health.backend_status));
    }

    Ok(())
}

async fn execute_stats(admin_url: String) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/stats", admin_url);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to connect to Admin API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        anyhow::bail!("API error ({}): {}", status, error_text);
    }

    let stats: StatsResponse = response
        .json()
        .await
        .context("Failed to parse response")?;

    println!("Overall Statistics:");
    print_output(&stats, OutputFormat::Table)?;
    Ok(())
}

async fn execute_config() -> anyhow::Result<()> {
    println!("Config command");
    println!("(Implementation pending - will show current configuration)");
    Ok(())
}
