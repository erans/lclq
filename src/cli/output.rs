// Output formatting utilities for CLI
use colored::*;
use serde::Serialize;
use tabled::{Table, Tabled};

/// Output format for CLI commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable table format
    Table,
    /// JSON format
    Json,
}

impl OutputFormat {
    /// Parse output format from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            _ => Self::Table,
        }
    }
}

/// Print data in the specified format
pub fn print_output<T>(data: &T, format: OutputFormat) -> anyhow::Result<()>
where
    T: Tabled + Serialize,
{
    match format {
        OutputFormat::Table => {
            let table = Table::new(std::iter::once(data)).to_string();
            println!("{}", table);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(data)?;
            println!("{}", json);
        }
    }
    Ok(())
}

/// Print a list of data in the specified format
pub fn print_list<T>(data: &[T], format: OutputFormat) -> anyhow::Result<()>
where
    T: Tabled + Serialize,
{
    match format {
        OutputFormat::Table => {
            if data.is_empty() {
                println!("{}", "No items found".yellow());
            } else {
                let table = Table::new(data).to_string();
                println!("{}", table);
            }
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(data)?;
            println!("{}", json);
        }
    }
    Ok(())
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("{} {}", "✓".green().bold(), message.green());
}

/// Print an error message
pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message.red());
}

/// Print a warning message
pub fn print_warning(message: &str) {
    println!("{} {}", "⚠".yellow().bold(), message.yellow());
}

/// Print an info message
pub fn print_info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message);
}
