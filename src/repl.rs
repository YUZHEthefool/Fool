//! REPL module for Fool Shell
//! Handles interactive shell with syntax highlighting and completions

#![allow(dead_code)]

use crate::ai::AiAgent;
use crate::config::Config;
use crate::executor::Executor;
use crate::history::{History, HistoryEntry};
use crate::parser::{ParseResult, Parser};
use anyhow::Result;
use crossterm::style::{Color, Stylize};
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{CompletionType, Config as RLConfig, Context, EditMode, Editor, Helper};
use std::borrow::Cow;
use std::collections::HashSet;

/// Shell prompt generator
pub struct Prompt;

impl Prompt {
    pub fn generate() -> String {
        let cwd = std::env::current_dir()
            .map(|p| {
                let home = dirs::home_dir();
                if let Some(ref home) = home {
                    if p.starts_with(home) {
                        format!("~{}", p.strip_prefix(home).unwrap().display())
                    } else {
                        p.display().to_string()
                    }
                } else {
                    p.display().to_string()
                }
            })
            .unwrap_or_else(|_| "?".to_string());

        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());

        // Colorful prompt: user@cwd >
        format!(
            "{} {} {} ",
            user.with(Color::Green).bold(),
            cwd.with(Color::Blue).bold(),
            "❯".with(Color::Magenta).bold()
        )
    }

    pub fn generate_plain() -> String {
        let cwd = std::env::current_dir()
            .map(|p| {
                let home = dirs::home_dir();
                if let Some(ref home) = home {
                    if p.starts_with(home) {
                        format!("~{}", p.strip_prefix(home).unwrap().display())
                    } else {
                        p.display().to_string()
                    }
                } else {
                    p.display().to_string()
                }
            })
            .unwrap_or_else(|_| "?".to_string());

        let user = std::env::var("USER").unwrap_or_else(|_| "user".to_string());
        format!("{} {} ❯ ", user, cwd)
    }
}

/// Known shell commands for highlighting
fn get_known_commands() -> HashSet<String> {
    let commands = vec![
        "ls", "cd", "pwd", "cat", "grep", "find", "echo", "rm", "cp", "mv",
        "mkdir", "rmdir", "touch", "chmod", "chown", "head", "tail", "less",
        "more", "vim", "nano", "git", "docker", "cargo", "npm", "python",
        "pip", "node", "make", "gcc", "g++", "rustc", "ssh", "scp", "curl",
        "wget", "tar", "zip", "unzip", "ps", "top", "htop", "kill", "man",
        "which", "whereis", "history", "export", "unset", "alias", "source",
        "exit", "clear", "help", "sudo", "apt", "yum", "dnf", "pacman",
    ];
    commands.into_iter().map(String::from).collect()
}

/// Custom helper for rustyline with highlighting and completion
pub struct FoolHelper {
    completer: FilenameCompleter,
    hinter: HistoryHinter,
    known_commands: HashSet<String>,
    ai_trigger: String,
}

impl FoolHelper {
    pub fn new(ai_trigger: String) -> Self {
        Self {
            completer: FilenameCompleter::new(),
            hinter: HistoryHinter::new(),
            known_commands: get_known_commands(),
            ai_trigger,
        }
    }
}

impl Completer for FoolHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Don't complete in AI mode
        if line.trim_start().starts_with(&self.ai_trigger) {
            return Ok((pos, vec![]));
        }

        // Use filename completer
        self.completer.complete(line, pos, ctx)
    }
}

impl Hinter for FoolHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        // Don't hint in AI mode
        if line.trim_start().starts_with(&self.ai_trigger) {
            return None;
        }

        self.hinter.hint(line, pos, ctx)
    }
}

impl Highlighter for FoolHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        let trimmed = line.trim_start();

        // AI mode highlighting
        if trimmed.starts_with(&self.ai_trigger) {
            return Cow::Owned(format!(
                "{}{}",
                self.ai_trigger.clone().with(Color::Yellow).bold(),
                &trimmed[self.ai_trigger.len()..].with(Color::Cyan)
            ));
        }

        // Simple syntax highlighting
        let mut result = String::new();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut current_word = String::new();
        let mut is_first_word = true;

        for c in line.chars() {
            if in_string {
                current_word.push(c);
                if c == string_char {
                    result.push_str(&current_word.clone().with(Color::Green).to_string());
                    current_word.clear();
                    in_string = false;
                }
            } else if c == '"' || c == '\'' {
                // Flush current word
                if !current_word.is_empty() {
                    result.push_str(&self.colorize_word(&current_word, is_first_word));
                    is_first_word = false;
                    current_word.clear();
                }
                in_string = true;
                string_char = c;
                current_word.push(c);
            } else if c.is_whitespace() || c == '|' || c == '>' || c == '<' {
                if !current_word.is_empty() {
                    result.push_str(&self.colorize_word(&current_word, is_first_word));
                    is_first_word = c == '|'; // Reset after pipe
                    current_word.clear();
                }
                if c == '|' || c == '>' || c == '<' {
                    result.push_str(&c.to_string().with(Color::Magenta).to_string());
                } else {
                    result.push(c);
                }
            } else {
                current_word.push(c);
            }
        }

        // Flush remaining
        if !current_word.is_empty() {
            if in_string {
                result.push_str(&current_word.with(Color::Green).to_string());
            } else {
                result.push_str(&self.colorize_word(&current_word, is_first_word));
            }
        }

        Cow::Owned(result)
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Borrowed(prompt)
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(hint.with(Color::DarkGrey).to_string())
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl FoolHelper {
    fn colorize_word(&self, word: &str, is_command: bool) -> String {
        if word.starts_with('-') {
            // Flag/option
            word.to_string().with(Color::Cyan).to_string()
        } else if word.starts_with('$') {
            // Variable
            word.to_string().with(Color::Yellow).to_string()
        } else if is_command {
            if self.known_commands.contains(word) {
                word.to_string().with(Color::Green).bold().to_string()
            } else {
                word.to_string().with(Color::White).to_string()
            }
        } else {
            word.to_string()
        }
    }
}

impl Validator for FoolHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();

        // Check for unclosed quotes
        let mut in_single = false;
        let mut in_double = false;
        let mut escape = false;

        for c in input.chars() {
            if escape {
                escape = false;
                continue;
            }
            match c {
                '\\' => escape = true,
                '\'' if !in_double => in_single = !in_single,
                '"' if !in_single => in_double = !in_double,
                _ => {}
            }
        }

        if in_single || in_double {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

impl Helper for FoolHelper {}

/// Main REPL structure
pub struct Repl {
    config: Config,
    parser: Parser,
    executor: Executor,
    history: History,
    ai_agent: AiAgent,
}

impl Repl {
    pub fn new(config: Config) -> Result<Self> {
        let parser = Parser::new(config.ai.trigger_prefix.clone());
        let executor = Executor::new();
        let history = History::new(
            config.history.file_path.clone(),
            config.history.max_entries,
        )?;
        let ai_agent = AiAgent::new(config.ai.clone());

        Ok(Self {
            config,
            parser,
            executor,
            history,
            ai_agent,
        })
    }

    /// Run the REPL loop
    pub async fn run(&mut self) -> Result<()> {
        // Configure rustyline
        let rl_config = RLConfig::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .build();

        let helper = FoolHelper::new(self.config.ai.trigger_prefix.clone());
        let mut rl: Editor<FoolHelper, DefaultHistory> = Editor::with_config(rl_config)?;
        rl.set_helper(Some(helper));

        // Load history into rustyline
        for cmd in self.history.get_all_commands() {
            let _ = rl.add_history_entry(cmd);
        }

        // Print welcome message
        self.print_welcome();

        loop {
            let prompt = Prompt::generate();

            match rl.readline(&prompt) {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // Add to rustyline history
                    let _ = rl.add_history_entry(line);

                    // Parse and execute
                    let result = self.parser.parse(line);
                    match result {
                        ParseResult::Commands(commands) => {
                            // Add to our history
                            let entry = HistoryEntry::new(line.to_string());
                            let _ = self.history.add(entry);

                            // Sync history to executor so history command works in pipelines
                            self.executor.set_history(self.history.get_all_commands().iter().map(|s| s.to_string()).collect());

                            // Execute commands
                            match self.executor.execute_pipeline(commands) {
                                Ok(exec_result) => {
                                    let _ = self.history.update_last_exit_code(exec_result.exit_code);
                                }
                                Err(e) => {
                                    eprintln!("{}: {}", "Error".with(Color::Red).bold(), e);
                                    let _ = self.history.update_last_exit_code(1);
                                }
                            }
                        }
                        ParseResult::AIQuery(query) => {
                            if query.is_empty() {
                                println!("{}", "Usage: ! <your question>".with(Color::Yellow));
                                continue;
                            }

                            // Add AI query to history
                            let entry = HistoryEntry::new(format!("! {}", query));
                            let _ = self.history.add(entry);

                            // Execute AI query
                            if !self.ai_agent.is_configured() {
                                eprintln!(
                                    "{}: AI not configured. Set FOOL_AI_KEY or OPENAI_API_KEY environment variable.",
                                    "Error".with(Color::Red).bold()
                                );
                                continue;
                            }

                            match self.ai_agent.query_stream(&query, &self.history).await {
                                Ok(_response) => {
                                    let _ = self.history.update_last_exit_code(0);
                                }
                                Err(e) => {
                                    eprintln!("{}: {}", "AI Error".with(Color::Red).bold(), e);
                                    let _ = self.history.update_last_exit_code(1);
                                }
                            }
                        }
                        ParseResult::Empty => {}
                        ParseResult::Error(e) => {
                            eprintln!("{}: {}", "Parse Error".with(Color::Red).bold(), e);
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl-C
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    // Ctrl-D
                    println!("exit");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    fn print_history(&self) {
        let entries = self.history.get_all_commands();
        if entries.is_empty() {
            println!("No history available");
            return;
        }

        for (i, cmd) in entries.iter().enumerate() {
            println!("{:5}  {}", i + 1, cmd);
        }
    }

    fn print_welcome(&self) {
        println!(
            "{}",
            r#"
  ███████╗ ██████╗  ██████╗ ██╗
  ██╔════╝██╔═══██╗██╔═══██╗██║
  █████╗  ██║   ██║██║   ██║██║
  ██╔══╝  ██║   ██║██║   ██║██║
  ██║     ╚██████╔╝╚██████╔╝███████╗
  ╚═╝      ╚═════╝  ╚═════╝ ╚══════╝
"#
            .with(Color::Cyan)
        );
        println!(
            "  {} v{}\n",
            "Fool Shell".with(Color::Green).bold(),
            env!("CARGO_PKG_VERSION")
        );
        println!(
            "  Type {} for help, {} to exit",
            "help".with(Color::Yellow),
            "exit".with(Color::Yellow)
        );
        println!(
            "  Use {} to ask AI for help\n",
            "! <question>".with(Color::Magenta)
        );
    }
}