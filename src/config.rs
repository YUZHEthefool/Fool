//! Configuration module for Fool Shell
//! Handles loading and parsing of config.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

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
        let mut config = if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {:?}", path))?;
            toml::from_str::<Config>(&content)
                .with_context(|| format!("Failed to parse config file: {:?}", path))?
        } else {
            // Return default config if file doesn't exist
            Config::default()
        };

        // M-04 & M-09: Validate and sanitize configuration values
        config.validate_and_fix();

        Ok(config)
    }

    /// Validate configuration values and fix invalid ones with defaults
    fn validate_and_fix(&mut self) {
        // M-04: Empty trigger_prefix causes all commands to be treated as AI queries
        if self.ai.trigger_prefix.is_empty() {
            eprintln!("Warning: ai.trigger_prefix cannot be empty, using default '!'");
            self.ai.trigger_prefix = "!".to_string();
        }

        // M-09: Validate max_entries to prevent excessive memory usage
        const MAX_HISTORY_ENTRIES: usize = 100_000;
        if self.history.max_entries > MAX_HISTORY_ENTRIES {
            eprintln!(
                "Warning: history.max_entries {} exceeds maximum {}, clamping",
                self.history.max_entries, MAX_HISTORY_ENTRIES
            );
            self.history.max_entries = MAX_HISTORY_ENTRIES;
        }
        if self.history.max_entries == 0 {
            eprintln!("Warning: history.max_entries cannot be 0, using default 10000");
            self.history.max_entries = 10000;
        }

        // M-09: Validate temperature (OpenAI API accepts 0.0 to 2.0)
        if self.ai.temperature < 0.0 || self.ai.temperature > 2.0 {
            eprintln!(
                "Warning: ai.temperature {} is out of range [0.0, 2.0], clamping",
                self.ai.temperature
            );
            self.ai.temperature = self.ai.temperature.clamp(0.0, 2.0);
        }

        // M-09: Validate context_lines to prevent excessive token usage
        const MAX_CONTEXT_LINES: usize = 1000;
        if self.ai.context_lines > MAX_CONTEXT_LINES {
            eprintln!(
                "Warning: ai.context_lines {} exceeds maximum {}, clamping",
                self.ai.context_lines, MAX_CONTEXT_LINES
            );
            self.ai.context_lines = MAX_CONTEXT_LINES;
        }
    }

    /// Save configuration to the default path
    #[allow(dead_code)] // Public API for config management
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path();
        self.save_to(&path)
    }

    /// Save configuration to a specific path
    /// Uses restricted permissions (0o600 on Unix) to protect sensitive data like API keys
    #[allow(dead_code)] // Public API for config management
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists with restricted permissions
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;

            // Set directory permissions to 0o700 on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                let _ = std::fs::set_permissions(parent, perms);
            }
        }

        let content = toml::to_string_pretty(self)
            .with_context(|| "Failed to serialize config")?;

        // Create file with restricted permissions (0o600 on Unix)
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);

        #[cfg(unix)]
        options.mode(0o600);

        let mut file = options
            .open(path)
            .with_context(|| format!("Failed to open config file for writing: {:?}", path))?;

        file.write_all(content.as_bytes())
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
