//! History module for Fool Shell
//! Manages command history with exit codes and timestamps

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

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

    #[allow(dead_code)] // Builder pattern for tests and future use
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = Some(code);
        self
    }

    #[allow(dead_code)] // Builder pattern for tests and future use
    pub fn with_stdout_summary(mut self, summary: String) -> Self {
        self.stdout_summary = Some(summary);
        self
    }
}

/// History manager
pub struct History {
    entries: VecDeque<HistoryEntry>,
    file_path: Option<PathBuf>, // None = memory-only mode
    max_entries: usize,
    entries_since_compact: usize, // Track entries added since last compaction
    pending_entry: bool,          // Track if last entry needs exit code update
}

impl History {
    /// Get the path to the lock file (sidecar file for coordinating access)
    fn get_lock_path(file_path: &Path) -> PathBuf {
        file_path.with_extension("lock")
    }

    /// Acquire exclusive lock on the sidecar lock file
    /// M-07: Using a sidecar lock file ensures that after rename operations,
    /// all processes still coordinate through the same persistent lock file
    fn acquire_lock(file_path: &Path) -> Result<File> {
        let lock_path = Self::get_lock_path(file_path);

        let mut lock_options = OpenOptions::new();
        lock_options.read(true).write(true).create(true);

        #[cfg(unix)]
        lock_options.mode(0o600);

        let lock_file = lock_options
            .open(&lock_path)
            .with_context(|| format!("Failed to open lock file: {:?}", lock_path))?;

        lock_file
            .lock_exclusive()
            .with_context(|| format!("Failed to acquire lock: {:?}", lock_path))?;

        Ok(lock_file)
    }

    pub fn new(file_path: String, max_entries: usize) -> Result<Self> {
        let file_path = Self::expand_path(&file_path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create history directory: {:?}", parent))?;
        }

        let mut history = Self {
            entries: VecDeque::with_capacity(max_entries),
            file_path: Some(file_path),
            max_entries,
            entries_since_compact: 0,
            pending_entry: false,
        };

        history.load()?;
        Ok(history)
    }

    /// Create a memory-only history (no file persistence)
    pub fn new_memory_only(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            file_path: None,
            max_entries,
            entries_since_compact: 0,
            pending_entry: false,
        }
    }

    fn expand_path(path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(path)
    }

    /// Load history from file
    fn load(&mut self) -> Result<()> {
        let file_path = match &self.file_path {
            Some(path) => path,
            None => return Ok(()), // Memory-only mode, skip loading
        };

        if !file_path.exists() {
            return Ok(());
        }

        let file = File::open(file_path)
            .with_context(|| format!("Failed to open history file: {:?}", file_path))?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                self.entries.push_back(entry);
                if self.entries.len() > self.max_entries {
                    self.entries.pop_front();
                }
            }
        }

        Ok(())
    }

    /// Add a new entry to history (without exit code initially)
    pub fn add(&mut self, entry: HistoryEntry) -> Result<()> {
        // Add to memory first
        self.entries.push_back(entry);
        if self.entries.len() > self.max_entries {
            self.entries.pop_front();
        }

        // Mark that we have a pending entry that will need its exit code updated
        self.pending_entry = true;

        // Track entries added since last compaction
        self.entries_since_compact += 1;

        // Compact file periodically to prevent unbounded growth
        // Trigger compaction every max_entries additions to keep file size reasonable
        if self.file_path.is_some() && self.entries_since_compact >= self.max_entries {
            self.compact()?;
            self.entries_since_compact = 0;
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
    #[allow(dead_code)] // Public API for history search
    pub fn search_prefix(&self, prefix: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.command.starts_with(prefix))
            .collect()
    }

    /// Search history by substring
    #[allow(dead_code)] // Used in tests
    pub fn search(&self, query: &str) -> Vec<&HistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.command.contains(query))
            .collect()
    }

    /// Get the last entry
    #[allow(dead_code)] // Public API for history access
    pub fn last(&self) -> Option<&HistoryEntry> {
        self.entries.back()
    }

    /// Get the last entry mutably
    pub fn last_mut(&mut self) -> Option<&mut HistoryEntry> {
        self.entries.back_mut()
    }

    /// Update the exit code of the last entry and write complete entry to disk
    pub fn update_last_exit_code(&mut self, code: i32) -> Result<()> {
        if let Some(entry) = self.entries.back_mut() {
            entry.exit_code = Some(code);

            // Now write the complete entry to disk (append-only)
            if let Some(file_path) = &self.file_path {
                if self.pending_entry {
                    // M-07: Acquire lock via sidecar lock file
                    let _lock_file = Self::acquire_lock(file_path)?;

                    let mut options = OpenOptions::new();
                    options.create(true).append(true);

                    // Set secure permissions (0o600) on Unix to protect command history
                    #[cfg(unix)]
                    options.mode(0o600);

                    let mut file = options.open(file_path).with_context(|| {
                        format!("Failed to open history file for writing: {:?}", file_path)
                    })?;

                    let json = serde_json::to_string(&entry)
                        .with_context(|| "Failed to serialize history entry")?;
                    writeln!(file, "{}", json).with_context(|| "Failed to write history entry")?;

                    // Ensure data is flushed before releasing lock
                    file.flush()
                        .with_context(|| "Failed to flush history entry")?;

                    // Lock released when _lock_file is dropped
                    self.pending_entry = false;
                }
            }
        }
        Ok(())
    }

    /// Get total entry count
    #[allow(dead_code)] // Public API
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    #[allow(dead_code)] // Public API
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear history
    #[allow(dead_code)] // Public API for history management
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        if let Some(file_path) = &self.file_path {
            if file_path.exists() {
                fs::remove_file(file_path)
                    .with_context(|| format!("Failed to remove history file: {:?}", file_path))?;
            }
        }
        Ok(())
    }

    /// Compact history file (remove old entries and rewrite)
    /// M-10 FIX: Skip pending entry to avoid duplicate writes
    /// M-07 FIX: Use sidecar lock file to prevent concurrent compaction conflicts
    pub fn compact(&mut self) -> Result<()> {
        let file_path = match &self.file_path {
            Some(path) => path,
            None => return Ok(()), // Memory-only mode, nothing to compact
        };

        // M-07: Acquire exclusive lock via sidecar lock file
        // Using a sidecar file ensures all processes coordinate through the same
        // persistent lock file even after rename operations
        let _lock_file = Self::acquire_lock(file_path)?;

        let temp_path = file_path.with_extension("tmp");

        {
            // Create temp file with secure permissions (0o600)
            let mut options = OpenOptions::new();
            options.create(true).write(true).truncate(true);

            #[cfg(unix)]
            options.mode(0o600);

            let mut file = options
                .open(&temp_path)
                .with_context(|| format!("Failed to create temp history file: {:?}", temp_path))?;

            // M-10 FIX: If there's a pending entry (last one without exit code),
            // skip it during compaction to avoid writing it twice
            let entries_to_write = if self.pending_entry && !self.entries.is_empty() {
                self.entries.len() - 1
            } else {
                self.entries.len()
            };

            for entry in self.entries.iter().take(entries_to_write) {
                let json = serde_json::to_string(entry)?;
                writeln!(file, "{}", json)?;
            }

            file.flush()?;
        }

        fs::rename(&temp_path, file_path).with_context(|| "Failed to rename temp history file")?;

        // Lock released when _lock_file is dropped
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

        history
            .add(HistoryEntry::new("git status".to_string()))
            .unwrap();
        history
            .add(HistoryEntry::new("git commit".to_string()))
            .unwrap();
        history
            .add(HistoryEntry::new("ls -la".to_string()))
            .unwrap();

        let results = history.search("git");
        assert_eq!(results.len(), 2);
    }
}
