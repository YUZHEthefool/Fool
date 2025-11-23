//! History module for Fool Shell
//! Manages command history with exit codes and timestamps

#![allow(dead_code)]

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// A single history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub command: String,
    pub exit_code: Option<i32>,
    pub timestamp: DateTime<Utc>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub stdout_summary: Option<String>,
}

impl HistoryEntry {
    pub fn new(command: String) -> Self {
        Self {
            command,
            exit_code: None,
            timestamp: Utc::now(),
            cwd: std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().to_string()),
            stdout_summary: None,
        }
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    pub fn with_stdout_summary(mut self, summary: String) -> Self {
        self.stdout_summary = Some(summary);
        self
    }
}

/// History manager
pub struct History {
    entries: VecDeque<HistoryEntry>,
    file_path: PathBuf,
    max_entries: usize,
}

impl History {
    pub fn new(file_path: String, max_entries: usize) -> Result<Self> {
        let file_path = Self::expand_path(&file_path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create history directory: {:?}", parent))?;
        }

        let mut history = Self {
            entries: VecDeque::with_capacity(max_entries),
            file_path,
            max_entries,
        };

        history.load()?;
        Ok(history)
    }

    fn expand_path(path: &str) -> PathBuf {
        if path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&path[2..]);
            }
        }
        PathBuf::from(path)
    }

    /// Load history from file
    fn load(&mut self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(());
        }

        let file = File::open(&self.file_path)
            .with_context(|| format!("Failed to open history file: {:?}", self.file_path))?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            if let Ok(line) = line {
                if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                    self.entries.push_back(entry);
                    if self.entries.len() > self.max_entries {
                        self.entries.pop_front();
                    }
                }
            }
        }

        Ok(())
    }

    /// Add a new entry to history
    pub fn add(&mut self, entry: HistoryEntry) -> Result<()> {
        // Append to file
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .with_context(|| format!("Failed to open history file for writing: {:?}", self.file_path))?;

        let json = serde_json::to_string(&entry)
            .with_context(|| "Failed to serialize history entry")?;
        writeln!(file, "{}", json)
            .with_context(|| "Failed to write history entry")?;

        // Add to memory
        self.entries.push_back(entry);
        if self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }

        Ok(())
    }

    /// Get recent entries for AI context
    pub fn get_recent(&self, count: usize) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .rev()
            .take(count)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get all entries (for rustyline history integration)
    pub fn get_all_commands(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.command.as_str()).collect()
    }

    /// Search history by prefix
    pub fn search_prefix(&self, prefix: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.command.starts_with(prefix))
            .collect()
    }

    /// Search history by substring
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.command.contains(query))
            .collect()
    }

    /// Get the last entry
    pub fn last(&self) -> Option<&HistoryEntry> {
        self.entries.back()
    }

    /// Update the exit code of the last entry
    pub fn update_last_exit_code(&mut self, code: i32) {
        if let Some(entry) = self.entries.back_mut() {
            entry.exit_code = Some(code);
        }
    }

    /// Get total entry count
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear history
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)
                .with_context(|| format!("Failed to remove history file: {:?}", self.file_path))?;
        }
        Ok(())
    }

    /// Compact history file (remove old entries)
    pub fn compact(&mut self) -> Result<()> {
        let temp_path = self.file_path.with_extension("tmp");

        {
            let mut file = File::create(&temp_path)
                .with_context(|| format!("Failed to create temp history file: {:?}", temp_path))?;

            for entry in &self.entries {
                let json = serde_json::to_string(entry)?;
                writeln!(file, "{}", json)?;
            }
        }

        fs::rename(&temp_path, &self.file_path)
            .with_context(|| "Failed to rename temp history file")?;

        Ok(())
    }

    /// Format history entries for AI context
    pub fn format_for_ai(&self, count: usize) -> Vec<serde_json::Value> {
        let recent = self.get_recent(count);
        let mut messages = Vec::new();

        for entry in recent {
            // Add user command
            messages.push(serde_json::json!({
                "role": "user",
                "content": entry.command
            }));

            // Add exit code as assistant response
            if let Some(code) = entry.exit_code {
                let response = if let Some(ref summary) = entry.stdout_summary {
                    format!("(Exit Code: {}) Output: {}", code, summary)
                } else {
                    format!("(Exit Code: {})", code)
                };
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": response
                }));
            }
        }

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_history_add_and_get() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let mut history = History::new(path.to_string_lossy().to_string(), 100).unwrap();

        let entry = HistoryEntry::new("ls -la".to_string()).with_exit_code(0);
        history.add(entry).unwrap();

        assert_eq!(history.len(), 1);
        assert_eq!(history.last().unwrap().command, "ls -la");
    }

    #[test]
    fn test_history_max_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let mut history = History::new(path.to_string_lossy().to_string(), 5).unwrap();

        for i in 0..10 {
            let entry = HistoryEntry::new(format!("cmd{}", i));
            history.add(entry).unwrap();
        }

        assert_eq!(history.len(), 5);
        assert_eq!(history.get_all_commands()[0], "cmd5");
    }

    #[test]
    fn test_history_search() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history");
        let mut history = History::new(path.to_string_lossy().to_string(), 100).unwrap();

        history.add(HistoryEntry::new("git status".to_string())).unwrap();
        history.add(HistoryEntry::new("git commit".to_string())).unwrap();
        history.add(HistoryEntry::new("ls -la".to_string())).unwrap();

        let results = history.search("git");
        assert_eq!(results.len(), 2);
    }
}
