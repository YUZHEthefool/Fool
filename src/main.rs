//! Fool Shell - A state-machine driven shell with native AI integration
//!
//! # Features
//! - State machine based command parsing
//! - Native AI integration via OpenAI API (triggered by !)
//! - Syntax highlighting and auto-completion
//! - Command history with context
//! - Pipe and redirection support

mod ai;
mod config;
mod executor;
mod history;
mod parser;
mod repl;

use anyhow::Result;
use config::Config;
use repl::Repl;
use std::env;

/// Print version information
fn print_version() {
    println!("Fool Shell v{}", env!("CARGO_PKG_VERSION"));
    println!("A state-machine driven shell with native AI integration");
}

/// Print usage information
fn print_usage() {
    println!("Usage: fool [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -h, --help       Print help information");
    println!("  -v, --version    Print version information");
    println!("  -c <command>     Execute a command and exit");
    println!("  --init-config    Generate default config file");
}

/// Initialize config file
fn init_config() -> Result<()> {
    let path = Config::default_path();
    if path.exists() {
        println!("Config file already exists at: {:?}", path);
        println!("To regenerate, delete the file first.");
        return Ok(());
    }

    // Create parent directory
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write default config
    let content = Config::generate_default_config();
    std::fs::write(&path, content)?;
    println!("Config file created at: {:?}", path);
    println!("Edit this file to configure AI and other settings.");
    Ok(())
}

/// Execute a single command (non-interactive mode)
async fn execute_command(cmd: &str, config: Config) -> Result<i32> {
    let parser = parser::Parser::new(config.ai.trigger_prefix.clone());
    let mut executor = executor::Executor::new();
    let history = history::History::new(
        config.history.file_path.clone(),
        config.history.max_entries,
    )?;
    let ai_agent = ai::AiAgent::new(config.ai.clone());

    let result = parser.parse(cmd);
    match result {
        parser::ParseResult::Commands(commands) => {
            match executor.execute_pipeline(commands) {
                Ok(exec_result) => Ok(exec_result.exit_code),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    Ok(1)
                }
            }
        }
        parser::ParseResult::AIQuery(query) => {
            if query.is_empty() {
                eprintln!("Usage: ! <your question>");
                return Ok(1);
            }

            if !ai_agent.is_configured() {
                eprintln!("Error: AI not configured. Set FOOL_AI_KEY or OPENAI_API_KEY.");
                return Ok(1);
            }

            match ai_agent.query_stream(&query, &history).await {
                Ok(_) => Ok(0),
                Err(e) => {
                    eprintln!("AI Error: {}", e);
                    Ok(1)
                }
            }
        }
        parser::ParseResult::Empty => Ok(0),
        parser::ParseResult::Error(e) => {
            eprintln!("Parse Error: {}", e);
            Ok(1)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment from .env if present
    let _ = dotenv::dotenv();

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            "-v" | "--version" => {
                print_version();
                return Ok(());
            }
            "--init-config" => {
                return init_config();
            }
            "-c" => {
                if args.len() < 3 {
                    eprintln!("Error: -c requires a command");
                    std::process::exit(1);
                }
                let config = Config::load()?;
                let cmd = args[2..].join(" ");
                let exit_code = execute_command(&cmd, config).await?;
                std::process::exit(exit_code);
            }
            _ => {
                eprintln!("Unknown option: {}", args[1]);
                print_usage();
                std::process::exit(1);
            }
        }
    }

    // Load configuration
    let config = Config::load()?;

    // Create and run REPL
    let mut repl = Repl::new(config)?;
    repl.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load_default() {
        let config = Config::default();
        assert_eq!(config.ai.trigger_prefix, "!");
    }
}
