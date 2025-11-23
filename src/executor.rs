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
    aliases: HashMap<String, String>,
    last_exit_code: i32,
}

impl Executor {
    pub fn new() -> Self {
        // Initialize with current environment
        let env_vars: HashMap<String, String> = std::env::vars().collect();

        Self {
            env_vars,
            aliases: HashMap::new(),
            last_exit_code: 0,
        }
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
                // History is handled by REPL
                Ok(ExecutionResult::success())
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
                // Source is complex, simplified here
                Ok(ExecutionResult::success())
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

    fn builtin_alias(&mut self, args: &[String]) -> Result<ExecutionResult> {
        if args.is_empty() {
            // List all aliases
            for (name, value) in &self.aliases {
                println!("alias {}='{}'", name, value);
            }
        } else {
            for arg in args {
                if let Some((name, value)) = arg.split_once('=') {
                    self.aliases.insert(name.to_string(), value.to_string());
                } else {
                    // Show specific alias
                    if let Some(value) = self.aliases.get(arg) {
                        println!("alias {}='{}'", arg, value);
                    }
                }
            }
        }
        Ok(ExecutionResult::success())
    }

    /// Execute external commands in a pipeline
    fn execute_external_pipeline(&mut self, commands: Vec<Command>) -> Result<ExecutionResult> {
        let mut children: Vec<Child> = Vec::new();
        let mut prev_stdout: Option<std::process::ChildStdout> = None;

        for (i, cmd) in commands.iter().enumerate() {
            let is_first = i == 0;
            let is_last = i == commands.len() - 1;

            // Resolve alias
            let program = self.aliases.get(&cmd.program)
                .cloned()
                .unwrap_or_else(|| cmd.program.clone());

            let mut process = ProcessCommand::new(&program);
            process.args(&cmd.args);

            // Set up environment
            for (key, value) in &self.env_vars {
                process.env(key, value);
            }

            // Set up stdin
            if is_first {
                if let Some(ref input_file) = cmd.stdin_redirect {
                    let file = File::open(input_file)
                        .with_context(|| format!("Cannot open file for input: {}", input_file))?;
                    process.stdin(Stdio::from(file));
                } else {
                    process.stdin(Stdio::inherit());
                }
            } else if let Some(stdout) = prev_stdout.take() {
                process.stdin(Stdio::from(stdout));
            }

            // Set up stdout
            if is_last {
                if let Some(ref output_file) = cmd.stdout_redirect {
                    let file = if cmd.stdout_append {
                        OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(output_file)
                    } else {
                        File::create(output_file)
                    }.with_context(|| format!("Cannot open file for output: {}", output_file))?;
                    process.stdout(Stdio::from(file));
                } else {
                    process.stdout(Stdio::inherit());
                }
            } else {
                process.stdout(Stdio::piped());
            }

            // Stderr always inherits
            process.stderr(Stdio::inherit());

            let mut child = process.spawn()
                .with_context(|| format!("Command not found: {}", program))?;

            // Save stdout for next command in pipeline
            if !is_last {
                prev_stdout = child.stdout.take();
            }

            children.push(child);
        }

        // Wait for all children and get the last exit status
        let mut last_status: Option<ExitStatus> = None;
        for mut child in children {
            last_status = Some(child.wait()?);
        }

        let exit_code = last_status
            .and_then(|s| s.code())
            .unwrap_or(1);

        self.last_exit_code = exit_code;

        Ok(ExecutionResult::with_code(exit_code))
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
}
