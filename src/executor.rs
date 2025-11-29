//! Command Executor module for Fool Shell
//! Handles process spawning, pipes, and redirections

#![allow(dead_code)]

use crate::parser::Command;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, ExitStatus, Stdio};

/// Result of command execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

impl ExecutionResult {
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stdout: None,
            stderr: None,
        }
    }

    pub fn with_code(code: i32) -> Self {
        Self {
            exit_code: code,
            stdout: None,
            stderr: None,
        }
    }
}

/// Built-in shell commands
pub enum BuiltinCommand {
    Cd,
    Exit,
    Export,
    Unset,
    History,
    Help,
    Clear,
    Pwd,
    Alias,
    Source,
}

impl BuiltinCommand {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cd" => Some(Self::Cd),
            "exit" | "quit" => Some(Self::Exit),
            "export" => Some(Self::Export),
            "unset" => Some(Self::Unset),
            "history" => Some(Self::History),
            "help" => Some(Self::Help),
            "clear" => Some(Self::Clear),
            "pwd" => Some(Self::Pwd),
            "alias" => Some(Self::Alias),
            "source" | "." => Some(Self::Source),
            _ => None,
        }
    }
}

/// Command executor
pub struct Executor {
    env_vars: HashMap<String, String>,
    aliases: HashMap<String, Vec<String>>,
    last_exit_code: i32,
    history_entries: Vec<String>, // Store history commands for display
    ai_trigger_prefix: String,    // M-03: Store AI trigger prefix for source command
}

impl Executor {
    pub fn new() -> Self {
        Self::with_ai_trigger("!".to_string())
    }

    /// Create executor with custom AI trigger prefix
    /// M-03: This allows source command to use the configured trigger
    pub fn with_ai_trigger(ai_trigger_prefix: String) -> Self {
        // Initialize with current environment
        let env_vars: HashMap<String, String> = std::env::vars().collect();

        Self {
            env_vars,
            aliases: HashMap::new(),
            last_exit_code: 0,
            history_entries: Vec::new(),
            ai_trigger_prefix,
        }
    }

    /// Set history entries for the history command
    pub fn set_history(&mut self, entries: Vec<String>) {
        self.history_entries = entries;
    }

    /// Get last exit code
    pub fn last_exit_code(&self) -> i32 {
        self.last_exit_code
    }

    /// Execute a pipeline of commands
    pub fn execute_pipeline(&mut self, commands: Vec<Command>) -> Result<ExecutionResult> {
        if commands.is_empty() {
            return Ok(ExecutionResult::success());
        }

        // Single command - check for builtins
        if commands.len() == 1 {
            let cmd = &commands[0];
            if let Some(builtin) = BuiltinCommand::from_str(&cmd.program) {
                return self.execute_builtin(builtin, cmd);
            }
        }

        // Execute pipeline
        let result = self.execute_external_pipeline(commands)?;
        self.last_exit_code = result.exit_code;
        Ok(result)
    }

    /// Execute a builtin command
    fn execute_builtin(&mut self, builtin: BuiltinCommand, cmd: &Command) -> Result<ExecutionResult> {
        match builtin {
            BuiltinCommand::Cd => {
                let path = cmd.args.first().map(|s| s.as_str()).unwrap_or("~");
                self.builtin_cd(path)
            }
            BuiltinCommand::Exit => {
                let code = cmd.args.first()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                std::process::exit(code);
            }
            BuiltinCommand::Export => {
                self.builtin_export(&cmd.args)
            }
            BuiltinCommand::Unset => {
                self.builtin_unset(&cmd.args)
            }
            BuiltinCommand::History => {
                self.builtin_history(&cmd.args)
            }
            BuiltinCommand::Help => {
                self.builtin_help()
            }
            BuiltinCommand::Clear => {
                self.builtin_clear()
            }
            BuiltinCommand::Pwd => {
                self.builtin_pwd()
            }
            BuiltinCommand::Alias => {
                self.builtin_alias(&cmd.args)
            }
            BuiltinCommand::Source => {
                self.builtin_source(&cmd.args)
            }
        }
    }

    fn builtin_cd(&mut self, path: &str) -> Result<ExecutionResult> {
        let expanded_path = if path == "~" || path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                if path == "~" {
                    home
                } else {
                    home.join(&path[2..])
                }
            } else {
                Path::new(path).to_path_buf()
            }
        } else if path == "-" {
            // Go to previous directory
            if let Some(oldpwd) = self.env_vars.get("OLDPWD") {
                Path::new(oldpwd).to_path_buf()
            } else {
                return Err(anyhow!("OLDPWD not set"));
            }
        } else {
            Path::new(path).to_path_buf()
        };

        // Save current directory as OLDPWD
        if let Ok(cwd) = std::env::current_dir() {
            self.env_vars.insert("OLDPWD".to_string(), cwd.to_string_lossy().to_string());
            std::env::set_var("OLDPWD", cwd);
        }

        std::env::set_current_dir(&expanded_path)
            .with_context(|| format!("cd: {}: No such file or directory", expanded_path.display()))?;

        // Update PWD
        if let Ok(cwd) = std::env::current_dir() {
            self.env_vars.insert("PWD".to_string(), cwd.to_string_lossy().to_string());
            std::env::set_var("PWD", cwd);
        }

        self.last_exit_code = 0;
        Ok(ExecutionResult::success())
    }

    fn builtin_export(&mut self, args: &[String]) -> Result<ExecutionResult> {
        for arg in args {
            if let Some((key, value)) = arg.split_once('=') {
                self.env_vars.insert(key.to_string(), value.to_string());
                std::env::set_var(key, value);
            } else {
                // Export existing variable
                if let Some(value) = self.env_vars.get(arg) {
                    std::env::set_var(arg, value);
                }
            }
        }
        self.last_exit_code = 0;
        Ok(ExecutionResult::success())
    }

    fn builtin_unset(&mut self, args: &[String]) -> Result<ExecutionResult> {
        for arg in args {
            self.env_vars.remove(arg);
            std::env::remove_var(arg);
        }
        self.last_exit_code = 0;
        Ok(ExecutionResult::success())
    }

    fn builtin_help(&self) -> Result<ExecutionResult> {
        println!("Fool Shell - A state-machine driven shell with AI integration");
        println!();
        println!("Built-in commands:");
        println!("  cd [dir]        Change directory");
        println!("  pwd             Print working directory");
        println!("  export VAR=val  Set environment variable");
        println!("  unset VAR       Unset environment variable");
        println!("  alias           Manage aliases");
        println!("  history         Show command history");
        println!("  clear           Clear the screen");
        println!("  help            Show this help");
        println!("  exit [code]     Exit the shell");
        println!();
        println!("AI Mode:");
        println!("  !query          Send a query to AI assistant");
        println!("  Example: ! how to find large files in Linux");
        Ok(ExecutionResult::success())
    }

    fn builtin_clear(&self) -> Result<ExecutionResult> {
        print!("\x1B[2J\x1B[1;1H");
        io::stdout().flush()?;
        Ok(ExecutionResult::success())
    }

    fn builtin_pwd(&self) -> Result<ExecutionResult> {
        let cwd = std::env::current_dir()?;
        println!("{}", cwd.display());
        Ok(ExecutionResult::success())
    }

    fn builtin_history(&self, _args: &[String]) -> Result<ExecutionResult> {
        if self.history_entries.is_empty() {
            println!("No history available");
        } else {
            for (i, cmd) in self.history_entries.iter().enumerate() {
                println!("{:5}  {}", i + 1, cmd);
            }
        }
        Ok(ExecutionResult::success())
    }

    fn builtin_alias(&mut self, args: &[String]) -> Result<ExecutionResult> {
        if args.is_empty() {
            // List all aliases
            for (name, tokens) in &self.aliases {
                println!("alias {}='{}'", name, tokens.join(" "));
            }
        } else {
            for arg in args {
                if let Some((name, value)) = arg.split_once('=') {
                    // Parse the alias value into tokens (handle quotes)
                    let tokens = self.parse_alias_value(value);
                    self.aliases.insert(name.to_string(), tokens);
                } else {
                    // Show specific alias
                    if let Some(tokens) = self.aliases.get(arg) {
                        println!("alias {}='{}'", arg, tokens.join(" "));
                    }
                }
            }
        }
        Ok(ExecutionResult::success())
    }

    fn builtin_source(&mut self, args: &[String]) -> Result<ExecutionResult> {
        if args.is_empty() {
            eprintln!("source: usage: source <filename>");
            self.last_exit_code = 1;
            return Ok(ExecutionResult::with_code(1));
        }

        let file_path = &args[0];

        // Expand ~ to home directory
        let expanded_path = if file_path.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&file_path[2..])
            } else {
                Path::new(file_path).to_path_buf()
            }
        } else {
            Path::new(file_path).to_path_buf()
        };

        // Read the file
        let content = match std::fs::read_to_string(&expanded_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("source: {}: {}", expanded_path.display(), e);
                self.last_exit_code = 1;
                return Ok(ExecutionResult::with_code(1));
            }
        };

        // M-03: Use the configured AI trigger prefix instead of hardcoded "!"
        let parser = crate::parser::Parser::new(self.ai_trigger_prefix.clone());
        let mut last_exit_code = 0;

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match parser.parse(line) {
                crate::parser::ParseResult::Commands(commands) => {
                    match self.execute_pipeline(commands) {
                        Ok(result) => {
                            last_exit_code = result.exit_code;
                        }
                        Err(e) => {
                            eprintln!("source: error executing '{}': {}", line, e);
                            last_exit_code = 1;
                        }
                    }
                }
                crate::parser::ParseResult::Empty => {}
                crate::parser::ParseResult::AIQuery(query) => {
                    // M-03: Warn user that AI queries in sourced files are not executed
                    eprintln!(
                        "source: AI query '{}{}' skipped (AI queries not supported in source files)",
                        self.ai_trigger_prefix, query
                    );
                }
                crate::parser::ParseResult::Error(e) => {
                    eprintln!("source: parse error in '{}': {}", line, e);
                    last_exit_code = 1;
                }
            }
        }

        self.last_exit_code = last_exit_code;
        Ok(ExecutionResult::with_code(last_exit_code))
    }

    /// Parse alias value into tokens, handling quotes
    fn parse_alias_value(&self, value: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut chars = value.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '\'' if !in_double_quote => {
                    in_single_quote = !in_single_quote;
                }
                '"' if !in_single_quote => {
                    in_double_quote = !in_double_quote;
                }
                ' ' | '\t' if !in_single_quote && !in_double_quote => {
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                }
                '\\' if in_double_quote => {
                    // Handle escape in double quotes
                    if let Some(&next) = chars.peek() {
                        if next == '"' || next == '\\' || next == '$' {
                            chars.next();
                            current_token.push(next);
                        } else {
                            current_token.push(c);
                        }
                    } else {
                        current_token.push(c);
                    }
                }
                _ => {
                    current_token.push(c);
                }
            }
        }

        if !current_token.is_empty() {
            tokens.push(current_token);
        }

        tokens
    }

    /// Clean up spawned children to prevent zombie processes
    fn cleanup_children(children: &mut Vec<Child>) {
        for child in children.iter_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        children.clear();
    }

    /// Execute external commands in a pipeline
    fn execute_external_pipeline(&mut self, commands: Vec<Command>) -> Result<ExecutionResult> {
        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, cmd) in commands.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == commands.len() - 1;

            // Resolve alias - expand tokens
            let (program, expanded_args) = if let Some(alias_tokens) = self.aliases.get(&cmd.program) {
                // First token is the program, rest are prepended args
                if alias_tokens.is_empty() {
                    (cmd.program.clone(), cmd.args.clone())
                } else {
                    let prog = alias_tokens[0].clone();
                    let mut args = alias_tokens[1..].to_vec();
                    args.extend(cmd.args.iter().cloned());
                    (prog, args)
                }
            } else {
                (cmd.program.clone(), cmd.args.clone())
            };

            // H-05: Warn about redirections on middle pipeline commands
            if !is_first && !is_last && (cmd.stdin_redirect.is_some() || cmd.stdout_redirect.is_some()) {
                eprintln!(
                    "Warning: redirections on middle pipeline command '{}' may not behave as expected",
                    program
                );
            }

            let mut process = ProcessCommand::new(&program);
            process.args(&expanded_args);

            // Set up environment
            for (key, value) in &self.env_vars {
                process.env(key, value);
            }

            // Set up stdin (redirect takes priority over pipe)
            if let Some(ref input_file) = cmd.stdin_redirect {
                // Explicit stdin redirect overrides pipe input
                let file = match File::open(input_file) {
                    Ok(f) => f,
                    Err(e) => {
                        Self::cleanup_children(&mut children);
                        return Err(anyhow!("Cannot open file for input: {}: {}", input_file, e));
                    }
                };
                process.stdin(Stdio::from(file));
            } else if is_first {
                process.stdin(Stdio::inherit());
            } else if let Some(stdout) = prev_stdout.take() {
                process.stdin(Stdio::from(stdout));
            }

            // Set up stdout (redirect takes priority over pipe)
            if let Some(ref output_file) = cmd.stdout_redirect {
                // Explicit stdout redirect
                let file = if cmd.stdout_append {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(output_file)
                } else {
                    File::create(output_file)
                };
                let file = match file {
                    Ok(f) => f,
                    Err(e) => {
                        Self::cleanup_children(&mut children);
                        return Err(anyhow!("Cannot open file for output: {}: {}", output_file, e));
                    }
                };
                process.stdout(Stdio::from(file));
            } else if is_last {
                // H-03 FIX: Allow the final command to inherit TTY for interactive programs
                // This enables programs like vim, top, less, ssh to work correctly
                process.stdout(Stdio::inherit());
            } else {
                process.stdout(Stdio::piped());
            }

            // Stderr always inherits
            process.stderr(Stdio::inherit());

            // H-04 FIX: Clean up already spawned children if spawn fails
            let mut child = match process.spawn() {
                Ok(c) => c,
                Err(e) => {
                    drop(prev_stdout); // Explicitly drop to avoid unused assignment warning
                    Self::cleanup_children(&mut children);
                    return Err(anyhow!("Command not found: {}: {}", program, e));
                }
            };

            // Save stdout for next command in pipeline
            if !is_last {
                prev_stdout = child.stdout.take();
            }

            children.push(child);
        }

        // H-03: Since we now inherit stdout for the last command (for TTY support),
        // we no longer capture stdout for history summary. This trade-off ensures
        // interactive programs like vim, top, less work correctly.

        // Wait for all children and get the last exit status
        let mut last_status: Option<ExitStatus> = None;
        for mut child in children {
            last_status = Some(child.wait()?);
        }

        let exit_code = last_status
            .and_then(|s| s.code())
            .unwrap_or(1);

        self.last_exit_code = exit_code;

        Ok(ExecutionResult {
            exit_code,
            stdout: None,
            stderr: None,
        })
    }

    /// Check if command is a builtin
    pub fn is_builtin(cmd: &str) -> bool {
        BuiltinCommand::from_str(cmd).is_some()
    }

    /// Get environment variable
    pub fn get_env(&self, key: &str) -> Option<&String> {
        self.env_vars.get(key)
    }

    /// Set environment variable
    pub fn set_env(&mut self, key: String, value: String) {
        std::env::set_var(&key, &value);
        self.env_vars.insert(key, value);
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin() {
        assert!(Executor::is_builtin("cd"));
        assert!(Executor::is_builtin("exit"));
        assert!(Executor::is_builtin("pwd"));
        assert!(!Executor::is_builtin("ls"));
        assert!(!Executor::is_builtin("grep"));
    }

    #[test]
    fn test_builtin_pwd() {
        let executor = Executor::new();
        let result = executor.builtin_pwd().unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_export_and_get_env() {
        let mut executor = Executor::new();
        executor.builtin_export(&["TEST_VAR=hello".to_string()]).unwrap();
        assert_eq!(executor.get_env("TEST_VAR"), Some(&"hello".to_string()));
    }

    #[test]
    fn test_alias_with_spaces() {
        let mut executor = Executor::new();

        // Test alias with multiple tokens
        executor.builtin_alias(&["ll=ls -la".to_string()]).unwrap();

        let alias_tokens = executor.aliases.get("ll").unwrap();
        assert_eq!(alias_tokens.len(), 2);
        assert_eq!(alias_tokens[0], "ls");
        assert_eq!(alias_tokens[1], "-la");
    }

    #[test]
    fn test_alias_with_quotes() {
        let mut executor = Executor::new();

        // Test alias with quoted string
        executor.builtin_alias(&["greet=echo 'hello world'".to_string()]).unwrap();

        let alias_tokens = executor.aliases.get("greet").unwrap();
        assert_eq!(alias_tokens.len(), 2);
        assert_eq!(alias_tokens[0], "echo");
        assert_eq!(alias_tokens[1], "hello world");
    }

    #[test]
    fn test_alias_expansion() {
        let mut executor = Executor::new();

        // Create an alias
        executor.builtin_alias(&["ll=ls -la".to_string()]).unwrap();

        // Simulate command that uses the alias
        let cmd = Command {
            program: "ll".to_string(),
            args: vec!["/tmp".to_string()],
            stdin_redirect: None,
            stdout_redirect: None,
            stdout_append: false,
        };

        // Get the alias
        let (program, expanded_args) = if let Some(alias_tokens) = executor.aliases.get(&cmd.program) {
            if alias_tokens.is_empty() {
                (cmd.program.clone(), cmd.args.clone())
            } else {
                let prog = alias_tokens[0].clone();
                let mut args = alias_tokens[1..].to_vec();
                args.extend(cmd.args.iter().cloned());
                (prog, args)
            }
        } else {
            (cmd.program.clone(), cmd.args.clone())
        };

        // Verify expansion
        assert_eq!(program, "ls");
        assert_eq!(expanded_args, vec!["-la", "/tmp"]);
    }
}
