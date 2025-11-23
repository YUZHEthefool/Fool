//! Configuration module for Fool Shell
//! Handles loading and parsing of config.toml

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_editor")]
    pub editor: String,
}

fn default_theme() -> String {
    "dracula".to_string()
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string())
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            editor: default_editor(),
        }
    }
}

/// History configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "default_history_path")]
    pub file_path: String,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
}

fn default_history_path() -> String {
    dirs::data_local_dir()
        .map(|p| p.join("fool").join("history").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.local/share/fool/history".to_string())
}

fn default_max_entries() -> usize {
    10000
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            file_path: default_history_path(),
            max_entries: default_max_entries(),
        }
    }
}

/// AI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_trigger_prefix")]
    pub trigger_prefix: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_context_lines")]
    pub context_lines: usize,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

fn default_trigger_prefix() -> String {
    "!".to_string()
}

fn default_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

fn default_temperature() -> f32 {
    0.7
}

fn default_context_lines() -> usize {
    10
}

fn default_system_prompt() -> String {
    "You are Fool, a helpful assistant running inside a command-line shell. \
     Be concise and provide direct answers. When suggesting commands, \
     provide them in a way that can be easily copied and executed."
        .to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            trigger_prefix: default_trigger_prefix(),
            api_base: default_api_base(),
            api_key: String::new(),
            model: default_model(),
            temperature: default_temperature(),
            context_lines: default_context_lines(),
            system_prompt: default_system_prompt(),
        }
    }
}

impl AiConfig {
    /// Get the API key, checking environment variable as fallback
    pub fn get_api_key(&self) -> Option<String> {
        if !self.api_key.is_empty() {
            Some(self.api_key.clone())
        } else {
            std::env::var("FOOL_AI_KEY")
                .or_else(|_| std::env::var("OPENAI_API_KEY"))
                .ok()
        }
    }
}

/// Root configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

impl Config {
    /// Get the default config file path
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("fool").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("~/.config/fool/config.toml"))
    }

    /// Load configuration from the default path or create default
    pub fn load() -> Result<Self> {
        let path = Self::default_path();
        Self::load_from(&path)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {:?}", path))?;
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {:?}", path))?;
            Ok(config)
        } else {
            // Return default config if file doesn't exist
            Ok(Config::default())
        }
    }

    /// Save configuration to the default path
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path();
        self.save_to(&path)
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;
        Ok(())
    }

    /// Generate a default config file with comments
    pub fn generate_default_config() -> String {
        r#"# Fool Shell Configuration

[ui]
theme = "dracula"          # Interface theme
editor = "vim"             # Default editor

[history]
file_path = "~/.local/share/fool/history"
max_entries = 10000        # Maximum history entries

[ai]
# AI trigger prefix, default is "!"
trigger_prefix = "!"

# OpenAI API configuration (compatible with OpenAI V1 format)
api_base = "https://api.openai.com/v1"
api_key = ""  # Or set FOOL_AI_KEY or OPENAI_API_KEY environment variable
model = "gpt-4o"
temperature = 0.7

# Context management
# How many recent interactions to include as context
context_lines = 10

# System prompt for AI
system_prompt = "You are Fool, a helpful assistant running inside a command-line shell. Be concise and provide direct answers. When suggesting commands, provide them in a way that can be easily copied and executed."
"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.ai.trigger_prefix, "!");
        assert_eq!(config.ai.context_lines, 10);
        assert_eq!(config.history.max_entries, 10000);
    }

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[ui]
theme = "monokai"

[ai]
model = "gpt-3.5-turbo"
context_lines = 20
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ui.theme, "monokai");
        assert_eq!(config.ai.model, "gpt-3.5-turbo");
        assert_eq!(config.ai.context_lines, 20);
    }
}
