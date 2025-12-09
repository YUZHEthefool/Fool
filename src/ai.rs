//! AI module for Fool Shell
//! Handles OpenAI API integration with streaming support

use crate::config::AiConfig;
use crate::history::History;
use anyhow::{anyhow, Context, Result};
use crossterm::{
    cursor, execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
};
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{stdout, Write};
use std::time::Duration;

/// OpenAI chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI chat completion request
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    stream: bool,
}

/// OpenAI streaming response chunk
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
    #[allow(dead_code)] // Part of API response structure
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<String>,
}

/// AI Agent for handling queries
pub struct AiAgent {
    client: Client,
    config: AiConfig,
}

impl AiAgent {
    /// Create a new AI agent with the given configuration
    /// M-05: Properly handle client build errors and preserve timeout settings
    pub fn new(config: AiConfig) -> Self {
        let client = Self::build_client();
        Self { client, config }
    }

    /// Build HTTP client with appropriate timeouts
    /// Falls back gracefully but preserves timeout configuration
    fn build_client() -> Client {
        let connect_timeout = Duration::from_secs(10);
        let request_timeout = Duration::from_secs(60);

        match Client::builder()
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .build()
        {
            Ok(client) => client,
            Err(e) => {
                // M-05: Log the error instead of silently swallowing it
                eprintln!(
                    "Warning: Failed to build HTTP client with custom settings: {}",
                    e
                );
                eprintln!("Warning: Using default client (timeouts may not be applied)");

                // Try a simpler configuration
                Client::builder().build().unwrap_or_else(|_| Client::new())
            }
        }
    }

    /// Check if AI is properly configured
    pub fn is_configured(&self) -> bool {
        self.config.get_api_key().is_some()
    }

    /// Build messages for the API request
    fn build_messages(&self, query: &str, history: &History) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Add system prompt
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: self.config.system_prompt.clone(),
        });

        // Add history context
        let history_messages = history.format_for_ai(self.config.context_lines);
        for msg in history_messages {
            if let (Some(role), Some(content)) = (msg["role"].as_str(), msg["content"].as_str()) {
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                });
            }
        }

        // Add current query
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: query.to_string(),
        });

        messages
    }

    /// Send a query and stream the response
    pub async fn query_stream(&self, query: &str, history: &History) -> Result<String> {
        let api_key = self.config.get_api_key()
            .ok_or_else(|| anyhow!("API key not configured. Set FOOL_AI_KEY or OPENAI_API_KEY environment variable, or configure api_key in config.toml"))?;

        let messages = self.build_messages(query, history);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: self.config.temperature,
            stream: true,
        };

        let url = format!("{}/chat/completions", self.config.api_base);

        // Show loading spinner
        print!("\r");
        execute!(
            stdout(),
            SetForegroundColor(Color::Cyan),
            Print("⠋ Thinking..."),
            ResetColor
        )?;
        stdout().flush()?;

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| "Failed to send request to AI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!(
                "API request failed with status {}: {}",
                status,
                body
            ));
        }

        // Clear loading message
        print!("\r");
        execute!(stdout(), cursor::MoveToColumn(0))?;
        print!("                    \r");

        // Process streaming response with proper SSE buffering
        let mut full_response = String::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new(); // Buffer for partial SSE frames

        // Print AI response header
        execute!(
            stdout(),
            SetForegroundColor(Color::Green),
            Print("AI: "),
            ResetColor
        )?;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.with_context(|| "Failed to read response chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            // Append to buffer to handle chunks that span HTTP boundaries
            buffer.push_str(&text);

            // M-01 FIX: Process complete SSE events using proper offset tracking
            // SSE events are separated by double newlines, but we process line by line
            while let Some(newline_pos) = buffer.find('\n') {
                // Extract the line (without the newline)
                let line = &buffer[..newline_pos];

                // Skip empty lines (event separators in SSE)
                if line.is_empty() || line == "\r" {
                    buffer = buffer[newline_pos + 1..].to_string();
                    continue;
                }

                // Process the line
                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        buffer.clear();
                        break;
                    }

                    // Try to parse as JSON - if it fails, the data might be incomplete
                    match serde_json::from_str::<StreamChunk>(data) {
                        Ok(sse_chunk) => {
                            for choice in sse_chunk.choices {
                                if let Some(content) = choice.delta.content {
                                    print!("{}", content);
                                    stdout().flush()?;
                                    full_response.push_str(&content);
                                }
                            }
                            // Successfully processed, remove from buffer
                            buffer = buffer[newline_pos + 1..].to_string();
                        }
                        Err(_) => {
                            // JSON parse failed - might be incomplete data
                            // Keep in buffer and wait for more data
                            break;
                        }
                    }
                } else {
                    // Not a data line (could be event:, id:, retry:, or other)
                    // Just skip it
                    buffer = buffer[newline_pos + 1..].to_string();
                }
            }
        }

        println!();
        Ok(full_response)
    }

    /// Send a query without streaming (for testing or simple use)
    #[allow(dead_code)] // Reserved for future non-streaming API usage
    pub async fn query(&self, query: &str, history: &History) -> Result<String> {
        let api_key = self
            .config
            .get_api_key()
            .ok_or_else(|| anyhow!("API key not configured"))?;

        let messages = self.build_messages(query, history);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            temperature: self.config.temperature,
            stream: false,
        };

        let url = format!("{}/chat/completions", self.config.api_base);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("API request failed: {} - {}", status, body));
        }

        let body: serde_json::Value = response.json().await?;
        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(content)
    }
}

/// Render markdown in terminal using termimad
#[allow(dead_code)] // Reserved for future markdown rendering feature
pub fn render_markdown(text: &str) {
    use termimad::MadSkin;

    let skin = MadSkin::default();
    skin.print_text(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_messages() {
        let config = AiConfig::default();
        let agent = AiAgent::new(config);
        let history = History::new("/tmp/fool_test_history".to_string(), 100).unwrap();

        let messages = agent.build_messages("test query", &history);
        assert!(!messages.is_empty());
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages.last().unwrap().role, "user");
        assert_eq!(messages.last().unwrap().content, "test query");
    }
}
